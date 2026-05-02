pub(crate) mod io;
pub(crate) mod lyrics;
pub(crate) mod metadata;

pub(crate) use io::sanitize_path_component;
pub(crate) use metadata::{TrackMetadata, write_audio_metadata};
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
};
use tokio::fs;
use uuid::Uuid;

use crate::{
    db::{self, wanted_status::WantedStatus},
    error::{AppError, AppResult},
    state::AppState,
};

pub(crate) async fn sync_album_wanted_status_from_tracks(
    state: &AppState,
    album_id: Uuid,
) -> AppResult<()> {
    let Some(album) = db::album::Entity::find_by_id(album_id)
        .one(&state.db)
        .await?
    else {
        return Err(AppError::not_found("album", Some(album_id.to_string())));
    };

    let tracks = db::track::Entity::find()
        .filter(db::track::Column::AlbumId.eq(album_id))
        .all(&state.db)
        .await?;

    let monitored_tracks = tracks
        .iter()
        .filter(|track| track.status != WantedStatus::Unmonitored)
        .count();

    let next_status = if monitored_tracks == 0 {
        Some(WantedStatus::Unmonitored)
    } else if monitored_tracks == tracks.len() && !tracks.is_empty() {
        Some(
            if tracks
                .iter()
                .all(|track| track.status == WantedStatus::Acquired)
            {
                WantedStatus::Acquired
            } else {
                WantedStatus::Wanted
            },
        )
    } else {
        None
    };

    if let Some(next_status) = next_status
        && next_status != album.wanted_status
    {
        let mut active = album.into_active_model();
        active.wanted_status = Set(next_status);
        active.update(&state.db).await?;
    }

    Ok(())
}

fn has_parent_dir_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn resolve_managed_track_path(music_root: &Path, stored_path: &str) -> Option<PathBuf> {
    let stored_path = Path::new(stored_path);

    if has_parent_dir_component(stored_path) {
        return None;
    }

    if stored_path.is_absolute() {
        return stored_path
            .starts_with(music_root)
            .then(|| stored_path.to_path_buf());
    }

    Some(music_root.join(stored_path))
}

async fn remove_file_if_exists(path: &Path) -> AppResult<bool> {
    if !fs::try_exists(path)
        .await
        .map_err(|err| AppError::filesystem("check file exists", path.display().to_string(), err))?
    {
        return Ok(false);
    }

    fs::remove_file(path)
        .await
        .map_err(|err| AppError::filesystem("remove file", path.display().to_string(), err))?;
    Ok(true)
}

async fn prune_empty_parent_dirs(path: &Path, music_root: &Path) -> AppResult<()> {
    let mut current = path.parent();

    while let Some(dir) = current {
        if dir == music_root {
            break;
        }

        let mut entries = match fs::read_dir(dir).await {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(err) => {
                return Err(AppError::filesystem(
                    "read directory",
                    dir.display().to_string(),
                    err,
                ));
            }
        };
        let is_empty = entries
            .next_entry()
            .await
            .map_err(|err| {
                AppError::filesystem("read directory entry", dir.display().to_string(), err)
            })?
            .is_none();

        if !is_empty {
            break;
        }

        match fs::remove_dir(dir).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(err) => {
                return Err(AppError::filesystem(
                    "remove directory",
                    dir.display().to_string(),
                    err,
                ));
            }
        }
        current = dir.parent();
    }

    Ok(())
}

/// Remove downloaded album files from disk.
pub(crate) async fn remove_downloaded_album_files(
    state: &AppState,
    album: &db::album::Model,
) -> AppResult<bool> {
    let tracks = db::track::Entity::find()
        .filter(db::track::Column::AlbumId.eq(album.id))
        .all(&state.db)
        .await?;

    let mut removed_any = false;
    let mut prunable_dirs = HashSet::new();

    for track in tracks {
        if let Some(file_path) = track.file_path.clone() {
            let Some(absolute_path) = resolve_managed_track_path(&state.music_root, &file_path)
            else {
                tracing::warn!(
                    album_id = %album.id,
                    track_id = %track.id,
                    file_path,
                    music_root = %state.music_root.display(),
                    "Skipping file removal for path outside managed music root"
                );
                continue;
            };

            let removed_audio = remove_file_if_exists(&absolute_path).await?;
            let removed_sidecar =
                remove_file_if_exists(&absolute_path.with_extension("lrc")).await?;

            if removed_audio || removed_sidecar {
                removed_any = true;
                prunable_dirs.insert(absolute_path);
            }
        }

        let was_acquired = track.status == WantedStatus::Acquired;
        let mut active = track.into_active_model();
        active.file_path = Set(None);
        active.root_folder_id = Set(None);
        if was_acquired {
            active.status = Set(WantedStatus::Wanted);
        }
        active.update(&state.db).await?;
    }

    for path in prunable_dirs {
        prune_empty_parent_dirs(&path, &state.music_root).await?;
    }

    Ok(removed_any)
}
