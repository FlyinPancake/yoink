use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use chrono::NaiveDate;
use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait,
    EntityTrait, IntoActiveModel, QueryFilter, TransactionTrait,
};
use tokio::fs;
use yoink_shared::{ImportConfirmation, ManualImportMode};

use crate::{
    db::{
        album, album_artist, album_type::AlbumType, artist, track, track_artist,
        wanted_status::WantedStatus,
    },
    error::{AppError, AppResult},
    services::downloads::sanitize_path_component,
    state::AppState,
    util::normalize,
};

use super::{
    discover::prepare_tracks,
    types::{DiscoveredAlbum, EXDEV_ERROR_CODE, PreparedTrack},
};

pub(super) async fn import_album_confirmation(
    state: &AppState,
    root_path: &Path,
    external_mode: Option<ManualImportMode>,
    album: &DiscoveredAlbum,
    confirmation: &ImportConfirmation,
) -> AppResult<usize> {
    let tracks = prepare_tracks(album);
    if tracks.is_empty() {
        return Err(AppError::validation(
            Some("preview_id"),
            "selected album has no importable audio files",
        ));
    }

    let transferred_paths = if let Some(mode) = external_mode {
        let target_dir = planned_album_directory(
            &state.music_root,
            &confirmation.artist_name,
            &confirmation.album_title,
            confirmation.year.as_deref(),
        );
        transfer_external_tracks(&tracks, &target_dir, mode).await?
    } else {
        Vec::new()
    };

    let tx = state.db.begin().await?;
    let persist_result = persist_imported_album(
        state,
        &tx,
        root_path,
        external_mode,
        confirmation,
        &tracks,
        &transferred_paths,
    )
    .await;

    match persist_result {
        Ok(artists_added) => {
            tx.commit().await?;
            state.notify_sse();
            Ok(artists_added)
        }
        Err(err) => {
            tx.rollback().await?;
            cleanup_transferred_files(&transferred_paths, &state.music_root).await;
            Err(err)
        }
    }
}

async fn persist_imported_album<C>(
    state: &AppState,
    db: &C,
    root_path: &Path,
    external_mode: Option<ManualImportMode>,
    confirmation: &ImportConfirmation,
    tracks: &[PreparedTrack],
    transferred_paths: &[PathBuf],
) -> AppResult<usize>
where
    C: ConnectionTrait,
{
    let mut artists_added = 0usize;

    let artist_id = if let Some(artist_id) = confirmation.artist_id {
        ensure_artist_exists(db, artist_id).await?;
        artist_id
    } else {
        let artist = artist::ActiveModel {
            name: Set(confirmation.artist_name.clone()),
            image_url: Set(None),
            bio: Set(None),
            monitored: Set(false),
            ..artist::ActiveModel::new()
        }
        .insert(db)
        .await?;
        artists_added += 1;
        artist.id
    };

    let album_id = if let Some(album_id) = confirmation.album_id {
        let existing = album::Entity::find_by_id(album_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::not_found("album", Some(album_id.to_string())))?;
        let mut active = existing.into_active_model();
        active.wanted_status = Set(WantedStatus::Acquired);
        active.update(db).await?;
        ensure_album_artist_link(db, album_id, artist_id).await?;
        album_id
    } else {
        let created_album = album::ActiveModel {
            title: Set(confirmation.album_title.clone()),
            album_type: Set(AlbumType::Unknown),
            release_date: Set(parse_release_year(confirmation.year.as_deref())),
            cover_url: Set(None),
            explicit: Set(false),
            wanted_status: Set(WantedStatus::Acquired),
            requested_quality: Set(None),
            ..album::ActiveModel::new()
        }
        .insert(db)
        .await?;

        album_artist::ActiveModel {
            album_id: Set(created_album.id),
            artist_id: Set(artist_id),
            priority: Set(0),
        }
        .insert(db)
        .await?;

        created_album.id
    };

    for (index, track) in tracks.iter().enumerate() {
        let relative_path = if external_mode.is_some() {
            transferred_paths
                .get(index)
                .and_then(|path| path.strip_prefix(&state.music_root).ok())
                .map(|path| path.to_string_lossy().to_string())
                .ok_or_else(|| {
                    AppError::validation(
                        Some("preview_id"),
                        "transferred file path escaped the managed library root",
                    )
                })?
        } else {
            track
                .source_path
                .strip_prefix(root_path)
                .map_err(|_| {
                    AppError::validation(
                        Some("preview_id"),
                        format!(
                            "file `{}` is outside the configured library root",
                            track.source_path.display()
                        ),
                    )
                })?
                .to_string_lossy()
                .to_string()
        };

        upsert_imported_track(db, album_id, artist_id, track, relative_path).await?;
    }

    Ok(artists_added)
}

async fn ensure_artist_exists<C>(db: &C, artist_id: uuid::Uuid) -> AppResult<()>
where
    C: ConnectionTrait,
{
    let exists = artist::Entity::find_by_id(artist_id)
        .one(db)
        .await?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(AppError::not_found("artist", Some(artist_id.to_string())))
    }
}

async fn ensure_album_artist_link<C>(
    db: &C,
    album_id: uuid::Uuid,
    artist_id: uuid::Uuid,
) -> AppResult<()>
where
    C: ConnectionTrait,
{
    let exists = album_artist::Entity::find()
        .filter(album_artist::Column::AlbumId.eq(album_id))
        .filter(album_artist::Column::ArtistId.eq(artist_id))
        .one(db)
        .await?
        .is_some();

    if !exists {
        album_artist::ActiveModel {
            album_id: Set(album_id),
            artist_id: Set(artist_id),
            priority: Set(0),
        }
        .insert(db)
        .await?;
    }

    Ok(())
}

async fn upsert_imported_track<C>(
    db: &C,
    album_id: uuid::Uuid,
    artist_id: uuid::Uuid,
    prepared: &PreparedTrack,
    relative_path: String,
) -> AppResult<()>
where
    C: ConnectionTrait,
{
    if let Some(existing) = track::Entity::find()
        .filter(track::Column::FilePath.eq(relative_path.clone()))
        .one(db)
        .await?
        && existing.album_id != album_id
    {
        return Err(AppError::conflict(format!(
            "file path `{relative_path}` is already attached to album {}",
            existing.album_id
        )));
    }

    let existing = match track::Entity::find()
        .filter(track::Column::AlbumId.eq(album_id))
        .filter(track::Column::FilePath.eq(relative_path.clone()))
        .one(db)
        .await?
    {
        Some(track) => Some(track),
        None => find_matching_track(db, album_id, prepared).await?,
    };

    let track_id = if let Some(existing) = existing {
        let mut active = existing.into_active_model();
        active.title = Set(prepared.title.clone());
        active.version = Set(None);
        active.disc_number = Set(prepared.disc_number);
        active.track_number = Set(prepared.track_number);
        active.duration = Set(prepared.duration_secs);
        active.explicit = Set(false);
        active.isrc = Set(prepared.isrc.clone());
        active.status = Set(WantedStatus::Acquired);
        active.file_path = Set(Some(relative_path));
        active.root_folder_id = Set(None);
        active.update(db).await?.id
    } else {
        track::ActiveModel {
            title: Set(prepared.title.clone()),
            version: Set(None),
            disc_number: Set(prepared.disc_number),
            track_number: Set(prepared.track_number),
            duration: Set(prepared.duration_secs),
            album_id: Set(album_id),
            explicit: Set(false),
            isrc: Set(prepared.isrc.clone()),
            root_folder_id: Set(None),
            status: Set(WantedStatus::Acquired),
            file_path: Set(Some(relative_path)),
            ..track::ActiveModel::new()
        }
        .insert(db)
        .await?
        .id
    };

    let has_artist = track_artist::Entity::find()
        .filter(track_artist::Column::TrackId.eq(track_id))
        .filter(track_artist::Column::ArtistId.eq(artist_id))
        .one(db)
        .await?
        .is_some();

    if !has_artist {
        track_artist::ActiveModel {
            track_id: Set(track_id),
            artist_id: Set(artist_id),
            priority: Set(0),
        }
        .insert(db)
        .await?;
    }

    Ok(())
}

async fn find_matching_track<C>(
    db: &C,
    album_id: uuid::Uuid,
    prepared: &PreparedTrack,
) -> AppResult<Option<track::Model>>
where
    C: ConnectionTrait,
{
    if let (Some(disc_number), Some(track_number)) = (prepared.disc_number, prepared.track_number)
        && let Some(existing) = track::Entity::find()
            .filter(track::Column::AlbumId.eq(album_id))
            .filter(track::Column::DiscNumber.eq(disc_number))
            .filter(track::Column::TrackNumber.eq(track_number))
            .one(db)
            .await?
    {
        return Ok(Some(existing));
    }

    let normalized_title = normalize(&prepared.title);
    let existing = track::Entity::find()
        .filter(track::Column::AlbumId.eq(album_id))
        .all(db)
        .await?
        .into_iter()
        .find(|candidate| normalize(&candidate.title) == normalized_title);

    Ok(existing)
}

fn parse_release_year(value: Option<&str>) -> Option<NaiveDate> {
    value
        .and_then(normalize_year)
        .and_then(|year| year.parse::<i32>().ok())
        .and_then(|year| NaiveDate::from_ymd_opt(year, 1, 1))
}

fn normalize_year(value: &str) -> Option<String> {
    if value.len() == 4
        && value.chars().all(|char| char.is_ascii_digit())
        && let Ok(year) = value.parse::<i32>()
        && (1900..=2100).contains(&year)
    {
        return Some(year.to_string());
    }

    None
}

fn planned_album_directory(
    music_root: &Path,
    artist_name: &str,
    album_title: &str,
    year: Option<&str>,
) -> PathBuf {
    let release_suffix = year
        .and_then(normalize_year)
        .unwrap_or_else(|| "Unknown".to_string());
    music_root
        .join(sanitize_path_component(artist_name))
        .join(format!(
            "{} ({})",
            sanitize_path_component(album_title),
            release_suffix
        ))
}

async fn transfer_external_tracks(
    tracks: &[PreparedTrack],
    target_dir: &Path,
    mode: ManualImportMode,
) -> AppResult<Vec<PathBuf>> {
    fs::create_dir_all(target_dir).await.map_err(|err| {
        AppError::filesystem(
            "create import target directory",
            target_dir.display().to_string(),
            err,
        )
    })?;

    let mut transferred = Vec::with_capacity(tracks.len());
    for track in tracks {
        let target_path = next_available_track_path(target_dir, track);
        transfer_one_external_track(&track.source_path, &target_path, mode).await?;
        transferred.push(target_path);
    }

    Ok(transferred)
}

fn next_available_track_path(target_dir: &Path, track: &PreparedTrack) -> PathBuf {
    let disc_prefix = match track.disc_number {
        Some(disc_number) => format!("{disc_number}-"),
        None => String::new(),
    };
    let track_number = track.track_number.unwrap_or(1);
    let ext = track
        .source_path
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("bin");

    let base_name = format!(
        "{disc_prefix}{track_number:02} - {}",
        sanitize_path_component(&track.title)
    );

    let mut candidate = target_dir.join(format!("{base_name}.{ext}"));
    let mut counter = 2usize;
    while candidate.exists() {
        candidate = target_dir.join(format!("{base_name} ({counter}).{ext}"));
        counter += 1;
    }
    candidate
}

async fn transfer_one_external_track(
    source_path: &Path,
    target_path: &Path,
    mode: ManualImportMode,
) -> AppResult<()> {
    match mode {
        ManualImportMode::Copy => {
            fs::copy(source_path, target_path).await.map_err(|err| {
                AppError::filesystem("copy import file", source_path.display().to_string(), err)
            })?;
        }
        ManualImportMode::Hardlink => match fs::hard_link(source_path, target_path).await {
            Ok(()) => {}
            Err(err) if err.raw_os_error() == Some(EXDEV_ERROR_CODE) => {
                fs::copy(source_path, target_path)
                    .await
                    .map_err(|copy_err| {
                        AppError::filesystem(
                            "copy import file after hardlink fallback",
                            source_path.display().to_string(),
                            copy_err,
                        )
                    })?;
            }
            Err(err) => {
                return Err(AppError::filesystem(
                    "hardlink import file",
                    source_path.display().to_string(),
                    err,
                ));
            }
        },
    }

    Ok(())
}

async fn cleanup_transferred_files(paths: &[PathBuf], music_root: &Path) {
    for path in paths {
        let _ = fs::remove_file(path).await;
        prune_empty_import_dirs(path, music_root).await;
    }
}

async fn prune_empty_import_dirs(path: &Path, music_root: &Path) {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir == music_root {
            break;
        }

        match fs::remove_dir(dir).await {
            Ok(()) => current = dir.parent(),
            Err(_) => break,
        }
    }
}
