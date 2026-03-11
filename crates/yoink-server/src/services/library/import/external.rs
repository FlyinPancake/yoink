use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use lofty::{
    file::{AudioFile, TaggedFileExt},
    prelude::Accessor,
    probe::Probe,
};
use tokio::fs;
use tracing::warn;
use uuid::Uuid;

use yoink_shared::{
    ImportConfirmation, ImportPreviewItem, ImportResultSummary, ManualImportMode, format_duration,
};

use crate::{
    db,
    error::{AppError, AppResult},
    services::downloads::sanitize_path_component,
    state::AppState,
    util::is_audio_extension,
};

use super::{
    ExternalAlbumDir, ensure_monitored_artist, import_local_album, match_album_candidates,
    md5_hash, sort_preview_items, split_album_folder_name,
};
use crate::services::library::{
    normalize_text, parse_release_year, sync::sync_artist_albums, update_wanted,
};

pub(crate) async fn preview_external_import(
    state: &AppState,
    source_path: &str,
) -> AppResult<Vec<ImportPreviewItem>> {
    let source = Path::new(source_path);
    if !fs::try_exists(source).await.unwrap_or(false) {
        return Err(AppError::not_found(
            "source_path",
            Some(source_path.to_string()),
        ));
    }

    let discovered = discover_external_albums(source).await?;
    if discovered.is_empty() {
        return Ok(Vec::new());
    }

    let artists = state.monitored_artists.read().await.clone();
    let albums = state.monitored_albums.read().await.clone();
    let artist_names_lower: HashMap<String, (Uuid, String)> = artists
        .iter()
        .map(|a| (normalize_text(&a.name), (a.id, a.name.clone())))
        .collect();

    let mut items = Vec::new();
    for local in &discovered {
        let item_id = format!(
            "{:x}",
            md5_hash(&format!("ext::{}/{}", local.artist_name, local.album_title))
        );
        let audio_count = local.audio_files.len();

        let (candidates, match_status) = match_album_candidates(
            &local.artist_name,
            &local.album_title,
            local.year.as_deref(),
            &artist_names_lower,
            &albums,
        );

        let already_imported = candidates.first().map(|c| c.acquired).unwrap_or(false);
        let selected_candidate = if !candidates.is_empty() && !already_imported {
            Some(0)
        } else {
            None
        };

        let relative_path = local
            .source_dir
            .strip_prefix(source)
            .unwrap_or(&local.source_dir)
            .to_string_lossy()
            .to_string();
        let relative_path = if relative_path.is_empty() {
            format!("{}/{}", local.artist_name, local.album_title)
        } else {
            relative_path
        };

        items.push(ImportPreviewItem {
            id: item_id,
            relative_path,
            discovered_artist: local.artist_name.clone(),
            discovered_album: local.album_title.clone(),
            discovered_year: local.year.clone(),
            match_status,
            candidates,
            selected_candidate,
            already_imported,
            audio_file_count: audio_count,
        });
    }

    sort_preview_items(&mut items);
    Ok(items)
}

pub(crate) async fn confirm_external_import(
    state: &AppState,
    source_path: &str,
    mode: ManualImportMode,
    items: Vec<ImportConfirmation>,
) -> AppResult<ImportResultSummary> {
    let source = Path::new(source_path);
    if !fs::try_exists(source).await.unwrap_or(false) {
        return Err(AppError::not_found(
            "source_path",
            Some(source_path.to_string()),
        ));
    }

    let discovered = discover_external_albums(source).await?;
    let mut imported = 0usize;
    let mut artists_added = 0usize;
    let mut failed = 0usize;
    let mut errors = Vec::new();
    let mut synced_artists = HashSet::new();

    for item in &items {
        let (artist_id, added_artist) = if let Some(aid) = item.artist_id {
            (aid, false)
        } else {
            match ensure_monitored_artist(state, &item.artist_name).await {
                Ok(Some((id, added))) => (id, added),
                Ok(None) => {
                    errors.push(format!(
                        "Could not find artist '{}' for album '{}'",
                        item.artist_name, item.album_title
                    ));
                    failed += 1;
                    continue;
                }
                Err(err) => {
                    errors.push(format!(
                        "Error resolving artist '{}': {}",
                        item.artist_name, err
                    ));
                    failed += 1;
                    continue;
                }
            }
        };

        if added_artist {
            artists_added += 1;
        }

        if !synced_artists.contains(&artist_id)
            && sync_artist_albums(state, artist_id).await.is_ok()
        {
            synced_artists.insert(artist_id);
        }

        let needle_artist = normalize_text(&item.artist_name);
        let needle_album = normalize_text(&item.album_title);
        let local_album = discovered.iter().find(|d| {
            normalize_text(&d.artist_name) == needle_artist
                && normalize_text(&d.album_title) == needle_album
        });

        let Some(local_album) = local_album else {
            errors.push(format!(
                "Source folder for '{}' / '{}' not found in scan",
                item.artist_name, item.album_title
            ));
            failed += 1;
            continue;
        };

        let year_suffix = item
            .year
            .as_deref()
            .and_then(parse_release_year)
            .or_else(|| local_album.year.clone());
        let album_folder = if let Some(ref y) = year_suffix {
            format!("{} ({})", item.album_title, y)
        } else {
            item.album_title.clone()
        };
        let target_dir = state
            .music_root
            .join(sanitize_path_component(&item.artist_name))
            .join(sanitize_path_component(&album_folder));

        if let Err(err) = fs::create_dir_all(&target_dir).await {
            errors.push(format!(
                "Failed to create directory {}: {}",
                target_dir.display(),
                err
            ));
            failed += 1;
            continue;
        }

        let mut album_ok = true;
        for src_file in &local_album.audio_files {
            let file_name = match src_file.file_name() {
                Some(n) => n,
                None => continue,
            };
            let dst_file = target_dir.join(file_name);

            if let Err(err) = import_file(src_file, &dst_file, mode).await {
                warn!(
                    src = %src_file.display(),
                    dst = %dst_file.display(),
                    error = %err,
                    "Failed to import file"
                );
                errors.push(format!("Failed to import {}: {}", src_file.display(), err));
                album_ok = false;
            }
        }

        if album_ok {
            let album_id = item.album_id.or_else(|| {
                let albums = state.monitored_albums.try_read().ok()?;
                albums
                    .iter()
                    .find(|a| {
                        (a.artist_id == artist_id || a.artist_ids.contains(&artist_id))
                            && normalize_text(&a.title) == needle_album
                    })
                    .map(|a| a.id)
            });

            if let Some(album_id) = album_id {
                persist_imported_tracks(state, album_id, &target_dir).await;

                let mut albums = state.monitored_albums.write().await;
                if let Some(album) = albums.iter_mut().find(|a| a.id == album_id) {
                    if !album.monitored {
                        album.monitored = true;
                    }
                    if !album.acquired {
                        album.acquired = true;
                    }
                    update_wanted(album);
                    if let Err(e) = db::update_album_flags(
                        &state.db,
                        album.id,
                        album.monitored,
                        album.acquired,
                        album.wanted,
                    )
                    .await
                    {
                        warn!(album_id = %album.id, error = %e, "Failed to update album flags after import");
                    }
                }
                imported += 1;
            } else if import_local_album(state, artist_id, &item.album_title, item.year.as_deref())
                .await?
            {
                imported += 1;
            } else {
                errors.push(format!(
                    "Could not match album '{}' by '{}' to any known album",
                    item.album_title, item.artist_name
                ));
                failed += 1;
            }
        } else {
            failed += 1;
        }
    }

    if imported > 0 {
        state.notify_sse();
    }

    Ok(ImportResultSummary {
        total_selected: items.len(),
        imported,
        artists_added,
        failed,
        errors,
    })
}

async fn import_file(src: &Path, dst: &Path, mode: ManualImportMode) -> AppResult<()> {
    if fs::try_exists(dst).await.unwrap_or(false) {
        fs::remove_file(dst).await.map_err(|err| {
            AppError::filesystem(
                "remove existing file before import",
                dst.display().to_string(),
                err,
            )
        })?;
    }

    match mode {
        ManualImportMode::Copy => {
            fs::copy(src, dst)
                .await
                .map_err(|err| AppError::filesystem("copy file", src.display().to_string(), err))?;
        }
        ManualImportMode::Hardlink => match fs::hard_link(src, dst).await {
            Ok(()) => {}
            Err(err) if is_cross_device_error(&err) => {
                warn!(
                    src = %src.display(),
                    dst = %dst.display(),
                    "Hardlink failed (cross-device), falling back to copy"
                );
                fs::copy(src, dst).await.map_err(|err| {
                    AppError::filesystem(
                        "copy file (hardlink fallback)",
                        src.display().to_string(),
                        err,
                    )
                })?;
            }
            Err(err) => {
                return Err(AppError::filesystem(
                    "hardlink file",
                    src.display().to_string(),
                    err,
                ));
            }
        },
    }
    Ok(())
}

fn is_cross_device_error(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(18)
}

async fn persist_imported_tracks(state: &AppState, album_id: Uuid, target_dir: &Path) {
    let mut entries = match fs::read_dir(target_dir).await {
        Ok(e) => e,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        if !ext.is_some_and(is_audio_extension) {
            continue;
        }

        let tag_data = match read_audio_tags(&path).await {
            Some(data) => data,
            None => continue,
        };

        let relative_path = path
            .strip_prefix(&state.music_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        let track_info = yoink_shared::TrackInfo {
            id: Uuid::now_v7(),
            title: tag_data.title,
            version: None,
            disc_number: tag_data.disc_number,
            track_number: tag_data.track_number,
            duration_secs: tag_data.duration_secs,
            duration_display: format_duration(tag_data.duration_secs),
            isrc: None,
            explicit: false,
            quality_override: None,
            track_artist: tag_data.track_artist,
            file_path: Some(relative_path),
            monitored: true,
            acquired: true,
        };

        if let Err(e) = db::upsert_track(&state.db, &track_info, album_id).await {
            warn!(file = %path.display(), error = %e, "Failed to persist imported track");
        }
    }
}

struct AudioTagData {
    title: String,
    track_artist: Option<String>,
    track_number: u32,
    disc_number: u32,
    duration_secs: u32,
}

async fn read_audio_tags(path: &Path) -> Option<AudioTagData> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || read_audio_tags_sync(&path))
        .await
        .ok()
        .flatten()
}

fn read_audio_tags_sync(path: &Path) -> Option<AudioTagData> {
    let tagged_file = Probe::open(path).ok()?.read().ok()?;
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())?;

    let title = tag.title().map(|s| s.to_string()).unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string()
    });

    let track_artist = tag.artist().map(|s| s.to_string());
    let track_number = tag.track().unwrap_or(0);
    let disc_number = tag.disk().unwrap_or(1);
    let duration_secs = tagged_file.properties().duration().as_secs() as u32;

    Some(AudioTagData {
        title,
        track_artist,
        track_number,
        disc_number,
        duration_secs,
    })
}

async fn discover_external_albums(source: &Path) -> AppResult<Vec<ExternalAlbumDir>> {
    let meta = fs::metadata(source).await.map_err(|err| {
        AppError::filesystem("stat external source", source.display().to_string(), err)
    })?;

    if !meta.is_dir() {
        return Err(AppError::validation(
            Some("source_path"),
            "Source path must be a directory",
        ));
    }

    let flat_audio = collect_audio_files(source).await;
    if !flat_audio.is_empty() {
        return Ok(infer_albums_from_tags(source, flat_audio).await);
    }

    let mut results = Vec::new();
    let mut top_entries = fs::read_dir(source).await.map_err(|err| {
        AppError::filesystem("read external source", source.display().to_string(), err)
    })?;

    while let Some(artist_entry) = top_entries.next_entry().await? {
        let artist_path = artist_entry.path();
        let is_dir = artist_entry
            .file_type()
            .await
            .map(|ft| ft.is_dir())
            .unwrap_or(false);
        if !is_dir {
            continue;
        }
        let Some(artist_name) = artist_path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if artist_name.starts_with('.') {
            continue;
        }

        let artist_audio = collect_audio_files(&artist_path).await;
        if !artist_audio.is_empty() {
            let inferred = infer_albums_from_tags(&artist_path, artist_audio).await;
            for mut album in inferred {
                if album.artist_name == "Unknown Artist" {
                    album.artist_name = artist_name.to_string();
                }
                results.push(album);
            }
            continue;
        }

        let mut album_entries = match fs::read_dir(&artist_path).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        while let Some(album_entry) = album_entries.next_entry().await.unwrap_or(None) {
            let album_path = album_entry.path();
            let is_dir = album_entry
                .file_type()
                .await
                .map(|ft| ft.is_dir())
                .unwrap_or(false);
            if !is_dir {
                continue;
            }
            let Some(album_folder) = album_path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if album_folder.starts_with('.') {
                continue;
            }

            let audio_files = collect_audio_files(&album_path).await;
            if audio_files.is_empty() {
                continue;
            }

            let (album_title, year) = split_album_folder_name(album_folder);
            results.push(ExternalAlbumDir {
                artist_name: artist_name.to_string(),
                album_title,
                year,
                source_dir: album_path,
                audio_files,
            });
        }
    }

    Ok(results)
}

async fn collect_audio_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut entries = match fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return files,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_audio_extension)
        {
            files.push(path);
        }
    }
    files.sort();
    files
}

async fn infer_albums_from_tags(
    source_dir: &Path,
    audio_files: Vec<PathBuf>,
) -> Vec<ExternalAlbumDir> {
    let mut groups: HashMap<(String, String), Vec<PathBuf>> = HashMap::new();

    for file in &audio_files {
        let (artist, album_name) = read_artist_album_from_tags(file, source_dir).await;
        groups
            .entry((artist, album_name))
            .or_default()
            .push(file.clone());
    }

    groups
        .into_iter()
        .map(|((artist, album), files)| ExternalAlbumDir {
            artist_name: artist,
            album_title: album,
            year: None,
            source_dir: source_dir.to_path_buf(),
            audio_files: files,
        })
        .collect()
}

async fn read_artist_album_from_tags(file: &Path, fallback_dir: &Path) -> (String, String) {
    let file = file.to_path_buf();
    let fallback_dir = fallback_dir.to_path_buf();
    tokio::task::spawn_blocking(move || read_artist_album_from_tags_sync(&file, &fallback_dir))
        .await
        .unwrap_or_else(|_| ("Unknown Artist".to_string(), "Unknown Album".to_string()))
}

fn read_artist_album_from_tags_sync(file: &Path, fallback_dir: &Path) -> (String, String) {
    let tagged = Probe::open(file).ok().and_then(|p| p.read().ok());

    match tagged {
        Some(ref tf) => {
            let tag = tf.primary_tag().or_else(|| tf.first_tag());
            let artist = tag
                .and_then(|t| t.artist().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unknown Artist".to_string());
            let album = tag
                .and_then(|t| t.album().map(|s| s.to_string()))
                .unwrap_or_else(|| {
                    fallback_dir
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Unknown Album")
                        .to_string()
                });
            (artist, album)
        }
        None => (
            "Unknown Artist".to_string(),
            fallback_dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown Album")
                .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{seed_album, seed_artist, test_app_state};

    #[tokio::test]
    async fn import_file_copy_creates_independent_file() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("song.flac");
        let dst = tmp.path().join("dest.flac");
        tokio::fs::write(&src, b"audio-data").await.unwrap();

        import_file(&src, &dst, ManualImportMode::Copy)
            .await
            .unwrap();

        assert!(tokio::fs::try_exists(&dst).await.unwrap());
        let content = tokio::fs::read(&dst).await.unwrap();
        assert_eq!(content, b"audio-data");
    }

    #[tokio::test]
    async fn import_file_hardlink_shares_inode() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("song.flac");
        let dst = tmp.path().join("dest.flac");
        tokio::fs::write(&src, b"audio-data").await.unwrap();

        import_file(&src, &dst, ManualImportMode::Hardlink)
            .await
            .unwrap();

        assert!(tokio::fs::try_exists(&dst).await.unwrap());
        let src_meta = tokio::fs::metadata(&src).await.unwrap();
        let dst_meta = tokio::fs::metadata(&dst).await.unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(src_meta.ino(), dst_meta.ino());
        }
    }

    #[test]
    fn cross_device_error_detection() {
        let err = std::io::Error::from_raw_os_error(18);
        assert!(is_cross_device_error(&err));

        let err = std::io::Error::from_raw_os_error(2);
        assert!(!is_cross_device_error(&err));
    }

    #[tokio::test]
    async fn discover_structured_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let album_dir = root.join("Test Artist").join("Test Album (2024)");
        tokio::fs::create_dir_all(&album_dir).await.unwrap();
        tokio::fs::write(album_dir.join("01 Track.flac"), b"fake-flac")
            .await
            .unwrap();
        tokio::fs::write(album_dir.join("02 Track.flac"), b"fake-flac")
            .await
            .unwrap();

        let result = discover_external_albums(root).await.unwrap();
        assert_eq!(result.len(), 1);

        let album = &result[0];
        assert_eq!(album.artist_name, "Test Artist");
        assert_eq!(album.album_title, "Test Album");
        assert_eq!(album.year, Some("2024".to_string()));
        assert_eq!(album.audio_files.len(), 2);
    }

    #[tokio::test]
    async fn discover_empty_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let result = discover_external_albums(tmp.path()).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn preview_external_import_with_match() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Test Artist").await;
        let album = seed_album(&state.db, artist.id, "Test Album").await;
        {
            let mut artists = state.monitored_artists.write().await;
            artists.push(artist.clone());
        }
        {
            let mut albums = state.monitored_albums.write().await;
            albums.push(album.clone());
        }

        let ext_tmp = tempfile::tempdir().unwrap();
        let ext_root = ext_tmp.path();
        let album_dir = ext_root.join("Test Artist").join("Test Album (2024)");
        tokio::fs::create_dir_all(&album_dir).await.unwrap();
        tokio::fs::write(album_dir.join("01 Song.flac"), b"fake")
            .await
            .unwrap();

        let preview = preview_external_import(&state, ext_root.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(preview.len(), 1);
        assert_eq!(preview[0].discovered_artist, "Test Artist");
        assert_eq!(preview[0].discovered_album, "Test Album");
        assert!(!preview[0].candidates.is_empty());
        assert_eq!(
            preview[0].match_status,
            yoink_shared::ImportMatchStatus::Matched
        );
    }

    #[tokio::test]
    async fn preview_external_import_unmatched() {
        let (state, _tmp) = test_app_state().await;

        let ext_tmp = tempfile::tempdir().unwrap();
        let ext_root = ext_tmp.path();
        let album_dir = ext_root.join("Unknown Band").join("Mystery Album (2024)");
        tokio::fs::create_dir_all(&album_dir).await.unwrap();
        tokio::fs::write(album_dir.join("01 Song.flac"), b"fake")
            .await
            .unwrap();

        let preview = preview_external_import(&state, ext_root.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(preview.len(), 1);
        assert_eq!(
            preview[0].match_status,
            yoink_shared::ImportMatchStatus::Unmatched
        );
        assert!(preview[0].candidates.is_empty());
    }
}
