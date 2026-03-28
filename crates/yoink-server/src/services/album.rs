use crate::{
    api::{Album, DownloadJob, MonitoredArtist, ProviderLink, TrackInfo},
    db::{
        self, album, album_artist, download_job, download_status::DownloadStatus,
        provider::Provider, quality::Quality, wanted_status::WantedStatus,
    },
    error::{AppError, AppResult},
    services,
    state::AppState,
};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    ColumnTrait, DatabaseConnection, EntityLoaderTrait, EntityTrait, IntoActiveModel, ModelTrait,
    QueryFilter,
};
use serde::Serialize;
use tracing::info;
use utoipa::ToSchema;
use uuid::Uuid;

use super::helpers;
use super::matching::AlbumMatchSuggestion;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ArtistWithPriority {
    #[serde(flatten)]
    pub artist: MonitoredArtist,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AlbumDetailResponse {
    album: Album,
    album_artists: Vec<ArtistWithPriority>,
    tracks: Vec<TrackInfo>,
    jobs: Vec<DownloadJob>,
    provider_links: Vec<ProviderLink>,
    album_match_suggestions: Vec<AlbumMatchSuggestion>,
    default_quality: Quality,
}

pub(crate) async fn get_album_details(
    state: &AppState,
    album_id: Uuid,
) -> AppResult<AlbumDetailResponse> {
    let album = album::Entity::find_by_id(album_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("album", Some(album_id.to_string())))?;

    let album_artists: Vec<_> = db::album_artist::Entity::find_by_album_ordered(album_id)
        .find_also_related(db::artist::Entity)
        .all(&state.db)
        .await?
        .into_iter()
        .filter_map(|(junction, artist)| match artist {
            Some(artist) => Some(ArtistWithPriority {
                artist: artist.into(),
                priority: junction.priority,
            }),
            None => {
                tracing::warn!(
                    album_id = %album_id,
                    artist_id = %junction.artist_id,
                    "Orphaned album_artist junction found, skipping"
                );
                None
            }
        })
        .collect();

    let tracks = get_album_tracks(&state.db, album.id).await?;

    let jobs = services::downloads::list_album_jobs(state, album.id).await?;

    let provider_links = album
        .find_related(db::album_provider_link::Entity)
        .all(&state.db)
        .await?
        .into_iter()
        .map(|l| l.into())
        .collect();

    let album_match_suggestions: Vec<AlbumMatchSuggestion> =
        db::album_match_suggestion::Entity::find_by_album(album_id)
            .all(&state.db)
            .await?
            .into_iter()
            .map(Into::into)
            .collect();

    Ok(AlbumDetailResponse {
        album: album.into(),
        album_artists,
        tracks,
        jobs,
        provider_links,
        album_match_suggestions,
        default_quality: state.default_quality,
    })
}

pub async fn get_album_tracks(
    db: &DatabaseConnection,
    album_id: Uuid,
) -> AppResult<Vec<TrackInfo>> {
    let tracks = db::track::Entity::find()
        .filter(db::track::Column::AlbumId.eq(album_id))
        .all(db)
        .await?
        .into_iter()
        .map(|t| t.into_ex().into())
        .collect();
    Ok(tracks)
}

pub(crate) async fn toggle_album_monitor(
    state: &AppState,
    album_id: Uuid,
    monitored: bool,
) -> AppResult<()> {
    if !monitored {
        services::downloads::prepare_album_for_unmonitor(state, album_id).await?;
    }

    let existing = album::Entity::find_by_id(album_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("album", Some(album_id.to_string())))?
        .wanted_status;

    let next_status = if monitored {
        if existing == WantedStatus::Unmonitored {
            WantedStatus::Wanted
        } else {
            existing
        }
    } else {
        WantedStatus::Unmonitored
    };

    album::Entity::update_many()
        .set(album::ActiveModel {
            id: NotSet,
            wanted_status: Set(next_status),
            ..Default::default()
        })
        .filter(album::Column::Id.eq(album_id))
        .exec(&state.db)
        .await?;

    let result = if monitored {
        monitor_all_tracks(state, album_id).await?
    } else {
        unmonitor_all_tracks(state, album_id).await?
    };

    info!(%album_id, monitored, "Toggled album monitored status, updated {} tracks", result.rows_affected);

    if monitored {
        services::downloads::enqueue_album_download(state, album_id).await?;
    }

    state.notify_sse();
    Ok(())
}

async fn monitor_all_tracks(
    state: &AppState,
    album_id: Uuid,
) -> Result<sea_orm::UpdateResult, AppError> {
    let result = db::track::Entity::update_many()
        .set(db::track::ActiveModel {
            id: NotSet,
            status: Set(WantedStatus::Wanted),
            ..Default::default()
        })
        .filter(db::track::Column::AlbumId.eq(album_id))
        .filter(db::track::Column::Status.eq(WantedStatus::Unmonitored))
        .exec(&state.db)
        .await?;
    Ok(result)
}

async fn unmonitor_all_tracks(
    state: &AppState,
    album_id: Uuid,
) -> Result<sea_orm::UpdateResult, AppError> {
    let result = db::track::Entity::update_many()
        .set(db::track::ActiveModel {
            id: NotSet,
            status: Set(WantedStatus::Unmonitored),
            ..Default::default()
        })
        .filter(db::track::Column::AlbumId.eq(album_id))
        .exec(&state.db)
        .await?;
    Ok(result)
}

pub(crate) async fn set_album_quality(
    state: &AppState,
    album_id: Uuid,
    quality: Option<Quality>,
) -> AppResult<()> {
    let album = album::Entity::load()
        .filter_by_id(album_id)
        .with(download_job::Entity)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("album", Some(album_id.to_string())))?
        .into_active_model();

    album
        .set_requested_quality(quality)
        .update(&state.db)
        .await?;

    // Update quality on queued/failed download jobs for this album
    if let Some(quality) = quality {
        download_job::Entity::update_many()
            .filter(download_job::Column::AlbumId.eq(album_id))
            .filter(
                download_job::Column::Status
                    .is_in([DownloadStatus::Queued, DownloadStatus::Failed]),
            )
            .set(download_job::ActiveModel {
                id: NotSet,
                quality: Set(quality),
                ..Default::default()
            })
            .exec(&state.db)
            .await?;
    }

    info!(%album_id, ?quality, "Updated album quality override");
    state.notify_sse();
    Ok(())
}

pub(crate) async fn merge_albums(
    state: &AppState,
    target_album_id: Uuid,
    source_album_id: Uuid,
    result_title: Option<String>,
    result_cover_url: Option<String>,
) -> AppResult<()> {
    if target_album_id == source_album_id {
        return Err(AppError::conflict(
            "target and source album must be different",
        ));
    }

    // Delegate to the existing merge logic in services::library
    services::merge_albums(
        state,
        target_album_id,
        source_album_id,
        result_title.as_deref(),
        result_cover_url.as_deref(),
    )
    .await?;

    // Find primary artist of target album for match suggestion recompute
    let primary_artist = album_artist::Entity::find_by_album_ordered(target_album_id)
        .one(&state.db)
        .await?;
    if let Some(aa) = primary_artist {
        helpers::spawn_recompute_artist_match_suggestions(state, aa.artist_id);
    }

    state.notify_sse();
    Ok(())
}

pub(crate) async fn remove_album_files(
    state: &AppState,
    album_id: Uuid,
    unmonitor: bool,
) -> AppResult<()> {
    let album_model = album::Entity::find_by_id(album_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("album", Some(album_id.to_string())))?;

    services::downloads::remove_downloaded_album_files(state, &album_model).await?;

    if unmonitor {
        toggle_album_monitor(state, album_id, false).await?;
    } else {
        album::Entity::update_many()
            .set(album::ActiveModel {
                id: NotSet,
                wanted_status: Set(WantedStatus::Wanted), // no longer acquired
                ..Default::default()
            })
            .filter(album::Column::Id.eq(album_id))
            .exec(&state.db)
            .await?;
    }

    // Delete completed download jobs for this album
    download_job::Entity::delete_many()
        .filter(download_job::Column::AlbumId.eq(album_id))
        .filter(download_job::Column::Status.eq(DownloadStatus::Completed))
        .exec(&state.db)
        .await?;

    info!(%album_id, unmonitor, "Removed downloaded album files");
    state.notify_sse();
    Ok(())
}

pub(crate) async fn add_album_artist(
    state: &AppState,
    album_id: Uuid,
    artist_id: Uuid,
) -> AppResult<()> {
    // Check if already linked
    let existing = album_artist::Entity::find()
        .filter(album_artist::Column::AlbumId.eq(album_id))
        .filter(album_artist::Column::ArtistId.eq(artist_id))
        .one(&state.db)
        .await?;

    if existing.is_none() {
        let current = album_artist::Entity::find_by_album_ordered(album_id)
            .all(&state.db)
            .await?;
        let next_priority = current.last().map(|aa| aa.priority + 1).unwrap_or(0);

        let model = album_artist::ActiveModel {
            album_id: Set(album_id),
            artist_id: Set(artist_id),
            priority: Set(next_priority),
        };
        model.insert(&state.db).await?;
    }

    state.notify_sse();
    Ok(())
}

pub(crate) async fn remove_album_artist(
    state: &AppState,
    album_id: Uuid,
    artist_id: Uuid,
) -> AppResult<()> {
    let current = album_artist::Entity::find_by_album_ordered(album_id)
        .all(&state.db)
        .await?;

    if current.len() <= 1 {
        return Err(AppError::conflict(
            "cannot remove the only artist from an album",
        ));
    }

    album_artist::Entity::delete_pair(album_id, artist_id)
        .exec(&state.db)
        .await?;

    state.notify_sse();
    Ok(())
}

pub(crate) async fn add_album(
    state: &AppState,
    provider: Provider,
    external_album_id: String,
    artist_external_id: String,
    artist_name: String,
    monitor_all: bool,
) -> AppResult<()> {
    let wanted_status = if monitor_all {
        WantedStatus::Wanted
    } else {
        WantedStatus::Unmonitored
    };

    let album_id = helpers::ensure_local_album(
        state,
        provider,
        &external_album_id,
        &artist_external_id,
        &artist_name,
        wanted_status,
    )
    .await?;

    // 4. Sync tracks from provider.
    super::sync_album_tracks(state, provider, &external_album_id, album_id).await?;

    info!(%album_id, %provider, %external_album_id, monitor_all, "Added album from search");
    state.notify_sse();
    Ok(())
}

#[cfg(test)]
mod tests {
    use sea_orm::{ActiveModelBehavior, ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};

    use super::{remove_album_files, toggle_album_monitor};
    use crate::{
        db::{
            self, album, album_type::AlbumType, download_job, download_status::DownloadStatus,
            provider::Provider, quality::Quality, track, wanted_status::WantedStatus,
        },
        error::AppError,
        test_support,
    };

    #[tokio::test]
    async fn toggle_album_monitor_updates_album_and_track_statuses() {
        let state = test_support::test_state().await;

        let album = album::ActiveModel {
            title: sea_orm::ActiveValue::Set("Test Album".to_string()),
            album_type: sea_orm::ActiveValue::Set(AlbumType::Album),
            release_date: sea_orm::ActiveValue::Set(None),
            cover_url: sea_orm::ActiveValue::Set(None),
            explicit: sea_orm::ActiveValue::Set(false),
            wanted_status: sea_orm::ActiveValue::Set(WantedStatus::Unmonitored),
            requested_quality: sea_orm::ActiveValue::Set(None),
            ..album::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album");

        let track = track::ActiveModel {
            title: sea_orm::ActiveValue::Set("Track 1".to_string()),
            version: sea_orm::ActiveValue::Set(None),
            disc_number: sea_orm::ActiveValue::Set(Some(1)),
            track_number: sea_orm::ActiveValue::Set(Some(1)),
            duration: sea_orm::ActiveValue::Set(Some(180)),
            album_id: sea_orm::ActiveValue::Set(album.id),
            explicit: sea_orm::ActiveValue::Set(false),
            isrc: sea_orm::ActiveValue::Set(None),
            root_folder_id: sea_orm::ActiveValue::Set(None),
            status: sea_orm::ActiveValue::Set(WantedStatus::Unmonitored),
            file_path: sea_orm::ActiveValue::Set(None),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert track");

        toggle_album_monitor(&state, album.id, true)
            .await
            .expect("monitor album");

        let album = db::album::Entity::find_by_id(album.id)
            .one(&state.db)
            .await
            .expect("reload album")
            .expect("album exists");
        let track = db::track::Entity::find_by_id(track.id)
            .one(&state.db)
            .await
            .expect("reload track")
            .expect("track exists");

        assert_eq!(album.wanted_status, WantedStatus::Wanted);
        assert_eq!(track.status, WantedStatus::Wanted);

        toggle_album_monitor(&state, album.id, false)
            .await
            .expect("unmonitor album");

        let album = db::album::Entity::find_by_id(album.id)
            .one(&state.db)
            .await
            .expect("reload album")
            .expect("album exists");
        let track = db::track::Entity::find_by_id(track.id)
            .one(&state.db)
            .await
            .expect("reload track")
            .expect("track exists");

        assert_eq!(album.wanted_status, WantedStatus::Unmonitored);
        assert_eq!(track.status, WantedStatus::Unmonitored);
    }

    #[tokio::test]
    async fn unmonitoring_album_cancels_queued_download_jobs() {
        let state = test_support::test_state().await;

        let album = album::ActiveModel {
            title: sea_orm::ActiveValue::Set("Queued Album".to_string()),
            album_type: sea_orm::ActiveValue::Set(AlbumType::Album),
            release_date: sea_orm::ActiveValue::Set(None),
            cover_url: sea_orm::ActiveValue::Set(None),
            explicit: sea_orm::ActiveValue::Set(false),
            wanted_status: sea_orm::ActiveValue::Set(WantedStatus::Wanted),
            requested_quality: sea_orm::ActiveValue::Set(None),
            ..album::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album");

        track::ActiveModel {
            title: sea_orm::ActiveValue::Set("Track 1".to_string()),
            version: sea_orm::ActiveValue::Set(None),
            disc_number: sea_orm::ActiveValue::Set(Some(1)),
            track_number: sea_orm::ActiveValue::Set(Some(1)),
            duration: sea_orm::ActiveValue::Set(Some(180)),
            album_id: sea_orm::ActiveValue::Set(album.id),
            explicit: sea_orm::ActiveValue::Set(false),
            isrc: sea_orm::ActiveValue::Set(None),
            root_folder_id: sea_orm::ActiveValue::Set(None),
            status: sea_orm::ActiveValue::Set(WantedStatus::Wanted),
            file_path: sea_orm::ActiveValue::Set(None),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert track");

        download_job::ActiveModel {
            album_id: sea_orm::ActiveValue::Set(album.id),
            track_id: sea_orm::ActiveValue::Set(None),
            source: sea_orm::ActiveValue::Set(Provider::Tidal),
            quality: sea_orm::ActiveValue::Set(Quality::Lossless),
            status: sea_orm::ActiveValue::Set(DownloadStatus::Queued),
            total_tracks: sea_orm::ActiveValue::Set(1),
            completed_tasks: sea_orm::ActiveValue::Set(0),
            error_message: sea_orm::ActiveValue::Set(None),
            ..download_job::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert queued job");

        toggle_album_monitor(&state, album.id, false)
            .await
            .expect("unmonitor album");

        assert!(
            db::download_job::Entity::find()
                .filter(db::download_job::Column::AlbumId.eq(album.id))
                .all(&state.db)
                .await
                .expect("load jobs")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn unmonitoring_album_conflicts_with_active_download_jobs() {
        let state = test_support::test_state().await;

        let album = album::ActiveModel {
            title: sea_orm::ActiveValue::Set("Busy Album".to_string()),
            album_type: sea_orm::ActiveValue::Set(AlbumType::Album),
            release_date: sea_orm::ActiveValue::Set(None),
            cover_url: sea_orm::ActiveValue::Set(None),
            explicit: sea_orm::ActiveValue::Set(false),
            wanted_status: sea_orm::ActiveValue::Set(WantedStatus::InProgress),
            requested_quality: sea_orm::ActiveValue::Set(None),
            ..album::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album");

        track::ActiveModel {
            title: sea_orm::ActiveValue::Set("Track 1".to_string()),
            version: sea_orm::ActiveValue::Set(None),
            disc_number: sea_orm::ActiveValue::Set(Some(1)),
            track_number: sea_orm::ActiveValue::Set(Some(1)),
            duration: sea_orm::ActiveValue::Set(Some(180)),
            album_id: sea_orm::ActiveValue::Set(album.id),
            explicit: sea_orm::ActiveValue::Set(false),
            isrc: sea_orm::ActiveValue::Set(None),
            root_folder_id: sea_orm::ActiveValue::Set(None),
            status: sea_orm::ActiveValue::Set(WantedStatus::Wanted),
            file_path: sea_orm::ActiveValue::Set(None),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert track");

        download_job::ActiveModel {
            album_id: sea_orm::ActiveValue::Set(album.id),
            track_id: sea_orm::ActiveValue::Set(None),
            source: sea_orm::ActiveValue::Set(Provider::Tidal),
            quality: sea_orm::ActiveValue::Set(Quality::Lossless),
            status: sea_orm::ActiveValue::Set(DownloadStatus::Downloading),
            total_tracks: sea_orm::ActiveValue::Set(1),
            completed_tasks: sea_orm::ActiveValue::Set(0),
            error_message: sea_orm::ActiveValue::Set(None),
            ..download_job::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert active job");

        let err = toggle_album_monitor(&state, album.id, false)
            .await
            .expect_err("active download should conflict");

        assert!(matches!(err, AppError::Conflict { .. }));
    }

    #[tokio::test]
    async fn remove_album_files_deletes_files_and_clears_track_state() {
        let music_root = tempfile::tempdir().expect("create music root");
        let state = test_support::test_state_with_music_root(music_root.path().to_path_buf()).await;

        let album = album::ActiveModel {
            title: sea_orm::ActiveValue::Set("Downloaded Album".to_string()),
            album_type: sea_orm::ActiveValue::Set(AlbumType::Album),
            release_date: sea_orm::ActiveValue::Set(None),
            cover_url: sea_orm::ActiveValue::Set(None),
            explicit: sea_orm::ActiveValue::Set(false),
            wanted_status: sea_orm::ActiveValue::Set(WantedStatus::Acquired),
            requested_quality: sea_orm::ActiveValue::Set(None),
            ..album::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album");

        let relative_file_path = "Test Artist/Downloaded Album/01 - Track.flac";
        let absolute_file_path = music_root.path().join(relative_file_path);
        tokio::fs::create_dir_all(
            absolute_file_path
                .parent()
                .expect("downloaded track should have parent directory"),
        )
        .await
        .expect("create album directory");
        tokio::fs::write(&absolute_file_path, b"audio")
            .await
            .expect("write downloaded track");
        tokio::fs::write(absolute_file_path.with_extension("lrc"), b"lyrics")
            .await
            .expect("write lyrics sidecar");

        let track = track::ActiveModel {
            title: sea_orm::ActiveValue::Set("Track 1".to_string()),
            version: sea_orm::ActiveValue::Set(None),
            disc_number: sea_orm::ActiveValue::Set(Some(1)),
            track_number: sea_orm::ActiveValue::Set(Some(1)),
            duration: sea_orm::ActiveValue::Set(Some(180)),
            album_id: sea_orm::ActiveValue::Set(album.id),
            explicit: sea_orm::ActiveValue::Set(false),
            isrc: sea_orm::ActiveValue::Set(None),
            root_folder_id: sea_orm::ActiveValue::Set(None),
            status: sea_orm::ActiveValue::Set(WantedStatus::Acquired),
            file_path: sea_orm::ActiveValue::Set(Some(relative_file_path.to_string())),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert downloaded track");

        download_job::ActiveModel {
            album_id: sea_orm::ActiveValue::Set(album.id),
            track_id: sea_orm::ActiveValue::Set(None),
            source: sea_orm::ActiveValue::Set(Provider::Tidal),
            quality: sea_orm::ActiveValue::Set(Quality::Lossless),
            status: sea_orm::ActiveValue::Set(DownloadStatus::Completed),
            total_tracks: sea_orm::ActiveValue::Set(1),
            completed_tasks: sea_orm::ActiveValue::Set(1),
            error_message: sea_orm::ActiveValue::Set(None),
            ..download_job::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert completed job");

        remove_album_files(&state, album.id, false)
            .await
            .expect("remove album files");

        let reloaded_album = db::album::Entity::find_by_id(album.id)
            .one(&state.db)
            .await
            .expect("reload album")
            .expect("album exists");
        let reloaded_track = db::track::Entity::find_by_id(track.id)
            .one(&state.db)
            .await
            .expect("reload track")
            .expect("track exists");

        assert_eq!(reloaded_album.wanted_status, WantedStatus::Wanted);
        assert_eq!(reloaded_track.status, WantedStatus::Wanted);
        assert_eq!(reloaded_track.file_path, None);
        assert_eq!(reloaded_track.root_folder_id, None);
        assert!(
            db::download_job::Entity::find()
                .filter(db::download_job::Column::AlbumId.eq(album.id))
                .all(&state.db)
                .await
                .expect("load album jobs")
                .is_empty()
        );
        assert!(
            !tokio::fs::try_exists(&absolute_file_path)
                .await
                .expect("check removed audio file")
        );
        assert!(
            !tokio::fs::try_exists(absolute_file_path.with_extension("lrc"))
                .await
                .expect("check removed lyrics sidecar")
        );
        assert!(
            !tokio::fs::try_exists(music_root.path().join("Test Artist/Downloaded Album"))
                .await
                .expect("check removed album directory")
        );
        assert!(
            !tokio::fs::try_exists(music_root.path().join("Test Artist"))
                .await
                .expect("check removed artist directory")
        );
    }

    #[tokio::test]
    async fn remove_and_unmonitor_handles_multiple_tracks_in_same_album_directory() {
        let music_root = tempfile::tempdir().expect("create music root");
        let state = test_support::test_state_with_music_root(music_root.path().to_path_buf()).await;

        let album = album::ActiveModel {
            title: sea_orm::ActiveValue::Set("For Old Times Sake EP".to_string()),
            album_type: sea_orm::ActiveValue::Set(AlbumType::Album),
            release_date: sea_orm::ActiveValue::Set(None),
            cover_url: sea_orm::ActiveValue::Set(None),
            explicit: sea_orm::ActiveValue::Set(false),
            wanted_status: sea_orm::ActiveValue::Set(WantedStatus::Acquired),
            requested_quality: sea_orm::ActiveValue::Set(None),
            ..album::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album");

        let album_dir = music_root
            .path()
            .join("K Motionz/For Old Times Sake EP (2024-02-02)");
        tokio::fs::create_dir_all(&album_dir)
            .await
            .expect("create album dir");

        for (track_number, title) in [(1, "Track One"), (2, "Track Two")] {
            let relative_file_path = format!(
                "K Motionz/For Old Times Sake EP (2024-02-02)/{:02} - {title}.flac",
                track_number
            );
            let absolute_file_path = music_root.path().join(&relative_file_path);
            tokio::fs::write(&absolute_file_path, b"audio")
                .await
                .expect("write track");

            track::ActiveModel {
                title: sea_orm::ActiveValue::Set(title.to_string()),
                version: sea_orm::ActiveValue::Set(None),
                disc_number: sea_orm::ActiveValue::Set(Some(1)),
                track_number: sea_orm::ActiveValue::Set(Some(track_number)),
                duration: sea_orm::ActiveValue::Set(Some(180)),
                album_id: sea_orm::ActiveValue::Set(album.id),
                explicit: sea_orm::ActiveValue::Set(false),
                isrc: sea_orm::ActiveValue::Set(None),
                root_folder_id: sea_orm::ActiveValue::Set(None),
                status: sea_orm::ActiveValue::Set(WantedStatus::Acquired),
                file_path: sea_orm::ActiveValue::Set(Some(relative_file_path)),
                ..track::ActiveModel::new()
            }
            .insert(&state.db)
            .await
            .expect("insert track");
        }

        remove_album_files(&state, album.id, true)
            .await
            .expect("remove and unmonitor album files");

        let reloaded_album = db::album::Entity::find_by_id(album.id)
            .one(&state.db)
            .await
            .expect("reload album")
            .expect("album exists");
        let tracks = db::track::Entity::find()
            .filter(db::track::Column::AlbumId.eq(album.id))
            .all(&state.db)
            .await
            .expect("reload tracks");

        assert_eq!(reloaded_album.wanted_status, WantedStatus::Unmonitored);
        assert_eq!(tracks.len(), 2);
        assert!(
            tracks
                .iter()
                .all(|track| track.status == WantedStatus::Unmonitored)
        );
        assert!(tracks.iter().all(|track| track.file_path.is_none()));
        assert!(
            !tokio::fs::try_exists(&album_dir)
                .await
                .expect("check removed shared album dir")
        );
    }
}
