use std::collections::{HashMap, HashSet};

use tokio::fs;
use uuid::Uuid;

use yoink_shared::{ImportConfirmation, ImportPreviewItem, ImportResultSummary};

use crate::{error::AppResult, state::AppState};

use super::super::sync::sync_artist_albums;
use super::{
    LocalAlbumDir, ensure_monitored_artist, import_local_album, match_album_candidates, md5_hash,
    sort_preview_items,
};
use crate::services::library::album_dir_has_downloaded_audio;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScanImportSummary {
    pub(crate) discovered_albums: usize,
    pub(crate) imported_albums: usize,
    pub(crate) artists_added: usize,
    pub(crate) unmatched_albums: usize,
}

pub(crate) async fn scan_and_import_library(state: &AppState) -> AppResult<ScanImportSummary> {
    let discovered = discover_local_albums(state).await?;
    if discovered.is_empty() {
        return Ok(ScanImportSummary {
            discovered_albums: 0,
            imported_albums: 0,
            artists_added: 0,
            unmatched_albums: 0,
        });
    }

    let mut imported_albums = 0usize;
    let mut artists_added = 0usize;
    let mut unmatched_albums = 0usize;
    let mut synced_artists = HashSet::new();

    for local in &discovered {
        let (artist_id, added_artist) = match ensure_monitored_artist(state, &local.artist_name)
            .await
        {
            Ok(Some(value)) => value,
            Ok(None) => {
                unmatched_albums += 1;
                continue;
            }
            Err(err) => {
                tracing::info!(artist = %local.artist_name, error = %err, "Failed to resolve artist during import");
                unmatched_albums += 1;
                continue;
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

        if import_local_album(state, artist_id, &local.album_title, local.year.as_deref()).await? {
            imported_albums += 1;
        } else {
            unmatched_albums += 1;
        }
    }

    if imported_albums > 0 {
        state.notify_sse();
    }

    Ok(ScanImportSummary {
        discovered_albums: discovered.len(),
        imported_albums,
        artists_added,
        unmatched_albums,
    })
}

pub(crate) async fn preview_import_library(state: &AppState) -> AppResult<Vec<ImportPreviewItem>> {
    let discovered = discover_local_albums(state).await?;
    if discovered.is_empty() {
        return Ok(Vec::new());
    }

    let artists = state.monitored_artists.read().await.clone();
    let albums = state.monitored_albums.read().await.clone();
    let artist_names_lower: HashMap<String, (Uuid, String)> = artists
        .iter()
        .map(|a| {
            (
                crate::services::library::normalize_text(&a.name),
                (a.id, a.name.clone()),
            )
        })
        .collect();

    let mut items = Vec::new();
    for local in &discovered {
        let item_id = format!(
            "{:x}",
            md5_hash(&format!("{}/{}", local.artist_name, local.album_title))
        );
        let relative_path = if let Some(year) = &local.year {
            format!("{}/{} ({})", local.artist_name, local.album_title, year)
        } else {
            format!("{}/{}", local.artist_name, local.album_title)
        };

        let album_dir = build_local_album_path(
            state,
            &local.artist_name,
            &local.album_title,
            local.year.as_deref(),
        );
        let audio_count = count_audio_files(&album_dir).await;

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

pub(crate) async fn confirm_import_library(
    state: &AppState,
    items: Vec<ImportConfirmation>,
) -> AppResult<ImportResultSummary> {
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

        if let Some(album_id) = item.album_id {
            let mut albums = state.monitored_albums.write().await;
            if let Some(album) = albums.iter_mut().find(|a| a.id == album_id) {
                if !album.monitored {
                    album.monitored = true;
                }
                if !album.acquired {
                    album.acquired = true;
                }
                crate::services::library::update_wanted(album);
                crate::db::update_album_flags(
                    &state.db,
                    album.id,
                    album.monitored,
                    album.acquired,
                    album.wanted,
                )
                .await?;
                imported += 1;
            } else {
                errors.push(format!(
                    "Album ID '{}' not found for '{}'",
                    album_id, item.album_title
                ));
                failed += 1;
            }
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
    }

    state.notify_sse();

    Ok(ImportResultSummary {
        total_selected: items.len(),
        imported,
        artists_added,
        failed,
        errors,
    })
}

async fn discover_local_albums(state: &AppState) -> AppResult<Vec<LocalAlbumDir>> {
    if !fs::try_exists(&state.music_root).await.unwrap_or(false) {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    let mut artist_dirs = fs::read_dir(&state.music_root).await.map_err(|err| {
        crate::error::AppError::filesystem(
            "read music root",
            state.music_root.display().to_string(),
            err,
        )
    })?;

    while let Some(artist_entry) = artist_dirs.next_entry().await? {
        let artist_path = artist_entry.path();
        if !artist_path.is_dir() {
            continue;
        }
        let Some(artist_name) = artist_path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };

        let mut album_dirs = match fs::read_dir(&artist_path).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        while let Some(album_entry) = album_dirs.next_entry().await.unwrap_or(None) {
            let album_path = album_entry.path();
            if !album_path.is_dir() || !album_dir_has_downloaded_audio(&album_path).await {
                continue;
            }
            let Some(album_folder) = album_path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let (album_title, year) = super::split_album_folder_name(album_folder);
            out.push(LocalAlbumDir {
                artist_name: artist_name.to_string(),
                album_title,
                year,
            });
        }
    }

    Ok(out)
}

fn build_local_album_path(
    state: &AppState,
    artist_name: &str,
    album_title: &str,
    year: Option<&str>,
) -> std::path::PathBuf {
    let folder = if let Some(y) = year {
        format!("{} ({})", album_title, y)
    } else {
        album_title.to_string()
    };
    state.music_root.join(artist_name).join(folder)
}

async fn count_audio_files(path: &std::path::Path) -> usize {
    let mut count = 0;
    if let Ok(mut entries) = fs::read_dir(path).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .map(|ext| {
                    ext.eq_ignore_ascii_case("flac")
                        || ext.eq_ignore_ascii_case("m4a")
                        || ext.eq_ignore_ascii_case("mp4")
                        || ext.eq_ignore_ascii_case("mp3")
                        || ext.eq_ignore_ascii_case("aac")
                        || ext.eq_ignore_ascii_case("ogg")
                        || ext.eq_ignore_ascii_case("opus")
                        || ext.eq_ignore_ascii_case("wav")
                })
                .unwrap_or(false)
            {
                count += 1;
            }
        }
    }
    count
}
