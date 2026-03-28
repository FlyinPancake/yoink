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

use crate::{
    api::{ImportConfirmation, ManualImportMode},
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
    use tempfile::tempdir;

    use crate::{
        api::{ImportConfirmation, ManualImportMode},
        db::{album_artist, track, track_artist, wanted_status::WantedStatus},
        test_support,
    };

    use super::{
        find_matching_track, next_available_track_path, normalize_year, parse_release_year,
        planned_album_directory, transfer_external_tracks,
    };
    use crate::services::library::import::shared::types::PreparedTrack;

    #[test]
    fn normalize_year_accepts_only_valid_years() {
        assert_eq!(normalize_year("2024").as_deref(), Some("2024"));
        assert_eq!(normalize_year("1899"), None);
        assert_eq!(normalize_year("2101"), None);
        assert_eq!(normalize_year("2024-01-01"), None);
    }

    #[test]
    fn parse_release_year_builds_first_day_of_year() {
        assert_eq!(
            parse_release_year(Some("2024")),
            chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
        );
        assert_eq!(parse_release_year(Some("not-a-year")), None);
        assert_eq!(parse_release_year(None), None);
    }

    #[test]
    fn planned_album_directory_sanitizes_names_and_falls_back_to_unknown_year() {
        let path = planned_album_directory(
            std::path::Path::new("/music"),
            "Artist/Name",
            "Album:Name",
            None,
        );

        assert_eq!(
            path,
            PathBuf::from("/music")
                .join("Artist_Name")
                .join("Album_Name (Unknown)")
        );
    }

    #[test]
    fn next_available_track_path_appends_counter_when_name_exists() {
        let dir = tempdir().expect("create dir");
        let track = PreparedTrack {
            source_path: dir.path().join("source.flac"),
            title: "Track Name".to_string(),
            disc_number: Some(1),
            track_number: Some(2),
            duration_secs: Some(180),
            isrc: None,
        };
        let existing = dir.path().join("1-02 - Track Name.flac");
        std::fs::write(&existing, b"audio").expect("write existing file");

        let next = next_available_track_path(dir.path(), &track);

        assert_eq!(next, dir.path().join("1-02 - Track Name (2).flac"));
    }

    #[tokio::test]
    async fn transfer_external_tracks_copies_files_into_target_dir() {
        let source_root = tempdir().expect("create source root");
        let target_root = tempdir().expect("create target root");
        let source_track = source_root.path().join("01 - Track.flac");
        std::fs::write(&source_track, b"audio").expect("write source file");
        let tracks = vec![PreparedTrack {
            source_path: source_track.clone(),
            title: "Track".to_string(),
            disc_number: None,
            track_number: Some(1),
            duration_secs: Some(180),
            isrc: None,
        }];

        let transferred =
            transfer_external_tracks(&tracks, target_root.path(), ManualImportMode::Copy)
                .await
                .expect("transfer tracks");

        assert_eq!(transferred.len(), 1);
        assert!(transferred[0].exists());
        assert_eq!(
            std::fs::read(&transferred[0]).expect("read transferred"),
            b"audio"
        );
        assert!(source_track.exists());
    }

    #[tokio::test]
    async fn find_matching_track_prefers_disc_and_track_number() {
        let state = test_support::test_state().await;
        let artist = test_support::seed_artist(&state, "Artist", true).await;
        let album = test_support::seed_album(&state, "Album", WantedStatus::Wanted).await;
        test_support::link_album_artist(&state, album.id, artist.id, 0).await;
        let existing =
            test_support::seed_track(&state, album.id, "Existing", 3, WantedStatus::Wanted).await;

        let prepared = PreparedTrack {
            source_path: PathBuf::from("/music/03 - New Title.flac"),
            title: "Different Title".to_string(),
            disc_number: Some(1),
            track_number: Some(3),
            duration_secs: Some(200),
            isrc: None,
        };

        let matched = find_matching_track(&state.db, album.id, &prepared)
            .await
            .expect("find matching track")
            .expect("matching track exists");

        assert_eq!(matched.id, existing.id);
    }

    #[tokio::test]
    async fn persist_imported_album_creates_new_artist_album_and_tracks() {
        let music_root = tempdir().expect("create music root");
        let state = test_support::test_state_with_music_root(music_root.path().to_path_buf()).await;
        let root_path = music_root.path();
        let tracks = vec![
            PreparedTrack {
                source_path: root_path.join("Artist/Album (2024)/01 - First.flac"),
                title: "First".to_string(),
                disc_number: Some(1),
                track_number: Some(1),
                duration_secs: Some(180),
                isrc: Some("ISRC001".to_string()),
            },
            PreparedTrack {
                source_path: root_path.join("Artist/Album (2024)/02 - Second.flac"),
                title: "Second".to_string(),
                disc_number: Some(1),
                track_number: Some(2),
                duration_secs: Some(200),
                isrc: Some("ISRC002".to_string()),
            },
        ];
        let confirmation = ImportConfirmation {
            preview_id: "preview-1".to_string(),
            artist_name: "Artist".to_string(),
            album_title: "Album".to_string(),
            year: Some("2024".to_string()),
            artist_id: None,
            album_id: None,
        };

        let tx = state.db.begin().await.expect("begin tx");
        let artists_added = super::persist_imported_album(
            &state,
            &tx,
            root_path,
            None,
            &confirmation,
            &tracks,
            &[],
        )
        .await
        .expect("persist imported album");
        tx.commit().await.expect("commit tx");

        assert_eq!(artists_added, 1);

        let artist = crate::db::artist::Entity::find()
            .one(&state.db)
            .await
            .expect("load artist")
            .expect("artist exists");
        let album = crate::db::album::Entity::find()
            .one(&state.db)
            .await
            .expect("load album")
            .expect("album exists");
        let stored_tracks = track::Entity::find()
            .filter(track::Column::AlbumId.eq(album.id))
            .all(&state.db)
            .await
            .expect("load tracks");
        let album_links = album_artist::Entity::find()
            .filter(album_artist::Column::AlbumId.eq(album.id))
            .all(&state.db)
            .await
            .expect("load album links");
        let track_links = track_artist::Entity::find()
            .all(&state.db)
            .await
            .expect("load track links");

        assert_eq!(artist.name, "Artist");
        assert!(!artist.monitored);
        assert_eq!(album.title, "Album");
        assert_eq!(album.wanted_status, WantedStatus::Acquired);
        assert_eq!(
            album.release_date,
            chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
        );
        assert_eq!(album_links.len(), 1);
        assert_eq!(stored_tracks.len(), 2);
        assert_eq!(track_links.len(), 2);
        assert!(
            stored_tracks
                .iter()
                .all(|track| track.status == WantedStatus::Acquired)
        );
        assert!(
            stored_tracks
                .iter()
                .any(|track| track.file_path.as_deref()
                    == Some("Artist/Album (2024)/01 - First.flac"))
        );
        assert!(stored_tracks.iter().any(
            |track| track.file_path.as_deref() == Some("Artist/Album (2024)/02 - Second.flac")
        ));
    }

    #[tokio::test]
    async fn persist_imported_album_updates_existing_album_and_track() {
        let music_root = tempdir().expect("create music root");
        let state = test_support::test_state_with_music_root(music_root.path().to_path_buf()).await;
        let root_path = music_root.path();
        let artist = test_support::seed_artist(&state, "Existing Artist", true).await;
        let album = test_support::seed_album(&state, "Existing Album", WantedStatus::Wanted).await;
        let existing_track =
            test_support::seed_track(&state, album.id, "Old Title", 1, WantedStatus::Wanted).await;
        let confirmation = ImportConfirmation {
            preview_id: "preview-2".to_string(),
            artist_name: artist.name.clone(),
            album_title: album.title.clone(),
            year: Some("2025".to_string()),
            artist_id: Some(artist.id),
            album_id: Some(album.id),
        };
        let tracks = vec![PreparedTrack {
            source_path: root_path
                .join("Existing Artist/Existing Album (2025)/01 - New Title.flac"),
            title: "New Title".to_string(),
            disc_number: Some(1),
            track_number: Some(1),
            duration_secs: Some(245),
            isrc: Some("NEWISRC".to_string()),
        }];

        let tx = state.db.begin().await.expect("begin tx");
        let artists_added = super::persist_imported_album(
            &state,
            &tx,
            root_path,
            None,
            &confirmation,
            &tracks,
            &[],
        )
        .await
        .expect("persist imported album");
        tx.commit().await.expect("commit tx");

        assert_eq!(artists_added, 0);

        let refreshed_album = crate::db::album::Entity::find_by_id(album.id)
            .one(&state.db)
            .await
            .expect("load album")
            .expect("album exists");
        let refreshed_track = track::Entity::find_by_id(existing_track.id)
            .one(&state.db)
            .await
            .expect("load track")
            .expect("track exists");
        let album_links = album_artist::Entity::find()
            .filter(album_artist::Column::AlbumId.eq(album.id))
            .all(&state.db)
            .await
            .expect("load album links");
        let track_links = track_artist::Entity::find()
            .filter(track_artist::Column::TrackId.eq(existing_track.id))
            .all(&state.db)
            .await
            .expect("load track links");

        assert_eq!(refreshed_album.wanted_status, WantedStatus::Acquired);
        assert_eq!(album_links.len(), 1);
        assert_eq!(album_links[0].artist_id, artist.id);
        assert_eq!(refreshed_track.title, "New Title");
        assert_eq!(refreshed_track.duration, Some(245));
        assert_eq!(refreshed_track.isrc.as_deref(), Some("NEWISRC"));
        assert_eq!(refreshed_track.status, WantedStatus::Acquired);
        assert_eq!(
            refreshed_track.file_path.as_deref(),
            Some("Existing Artist/Existing Album (2025)/01 - New Title.flac")
        );
        assert_eq!(track_links.len(), 1);
        assert_eq!(track_links[0].artist_id, artist.id);
    }
}
