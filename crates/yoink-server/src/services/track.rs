use std::collections::{HashMap, HashSet};

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder,
};
use tracing::{info, warn};
use uuid::Uuid;
use yoink_shared::LibraryTrack;

use crate::{
    db::{
        self, album, download_job, download_status::DownloadStatus, provider::Provider,
        quality::Quality, track, track_provider_link, wanted_status::WantedStatus,
    },
    error::{AppError, AppResult},
    services,
    state::AppState,
};

use super::helpers;

pub(crate) async fn list_library_tracks(state: &AppState) -> AppResult<Vec<LibraryTrack>> {
    let tracks_with_albums = track::Entity::find()
        .find_also_related(album::Entity)
        .order_by_asc(track::Column::CreatedAt)
        .all(&state.db)
        .await?;

    if tracks_with_albums.is_empty() {
        return Ok(Vec::new());
    }

    let album_ids: Vec<Uuid> = tracks_with_albums
        .iter()
        .filter_map(|(_, album)| album.as_ref().map(|album| album.id))
        .collect();

    let album_artists = db::album_artist::Entity::find()
        .filter(db::album_artist::Column::AlbumId.is_in(album_ids.iter().copied()))
        .order_by_asc(db::album_artist::Column::Priority)
        .all(&state.db)
        .await?;

    let mut primary_artist_by_album = HashMap::new();
    let mut artist_ids = HashSet::new();

    for album_artist in album_artists {
        primary_artist_by_album
            .entry(album_artist.album_id)
            .or_insert(album_artist.artist_id);
        artist_ids.insert(album_artist.artist_id);
    }

    let artists_by_id: HashMap<Uuid, db::artist::Model> = if artist_ids.is_empty() {
        HashMap::new()
    } else {
        db::artist::Entity::find()
            .filter(db::artist::Column::Id.is_in(artist_ids))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|artist| (artist.id, artist))
            .collect()
    };

    let mut library_tracks = Vec::with_capacity(tracks_with_albums.len());

    for (track, album) in tracks_with_albums {
        let Some(album) = album else {
            warn!(track_id = %track.id, "Track without album found, skipping library track row");
            continue;
        };

        let Some(artist_id) = primary_artist_by_album.get(&album.id).copied() else {
            warn!(track_id = %track.id, album_id = %album.id, "Album without primary artist found, skipping library track row");
            continue;
        };

        let Some(artist) = artists_by_id.get(&artist_id) else {
            warn!(track_id = %track.id, album_id = %album.id, artist_id = %artist_id, "Primary artist missing for album, skipping library track row");
            continue;
        };

        library_tracks.push(LibraryTrack {
            track: track.into(),
            album_id: album.id,
            album_title: album.title,
            album_cover_url: album.cover_url,
            artist_id,
            artist_name: artist.name.clone(),
        });
    }

    Ok(library_tracks)
}

pub(crate) async fn add_track(
    state: &AppState,
    provider: Provider,
    external_track_id: String,
    external_album_id: String,
    artist_external_id: String,
    artist_name: String,
) -> AppResult<()> {
    let album_id = helpers::ensure_local_album(
        state,
        provider,
        &external_album_id,
        &artist_external_id,
        &artist_name,
        WantedStatus::Unmonitored,
    )
    .await?;

    // 4. Sync tracks from provider.
    super::sync_album_tracks(state, provider, &external_album_id, album_id).await?;

    // 5. Mark the target track as wanted.
    let target_link = track_provider_link::Entity::find()
        .filter(track_provider_link::Column::ProviderTrackId.eq(&external_track_id))
        .filter(track_provider_link::Column::Provider.eq(provider))
        .one(&state.db)
        .await?;

    if let Some(link) = target_link
        && let Some(found) = track::Entity::find_by_id(link.track_id)
            .one(&state.db)
            .await?
    {
        let mut model: track::ActiveModel = found.into();
        model.status = Set(WantedStatus::Wanted);
        model.update(&state.db).await?;
        services::downloads::enqueue_track_download(state, link.track_id).await?;
    }

    info!(%album_id, %provider, %external_track_id, "Added track from search");
    state.notify_sse();
    Ok(())
}

pub(crate) async fn toggle_track_monitor(
    state: &AppState,
    track_id: Uuid,
    album_id: Uuid,
    monitored: bool,
) -> AppResult<()> {
    let track = track::Entity::find_by_id(track_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("track", Some(track_id.to_string())))?;

    let next_status = if monitored {
        if track.file_path.is_some() || track.status == WantedStatus::Acquired {
            WantedStatus::Acquired
        } else {
            WantedStatus::Wanted
        }
    } else {
        WantedStatus::Unmonitored
    };
    let should_enqueue = monitored && next_status == WantedStatus::Wanted;

    let mut active = track.into_active_model();
    active.status = Set(next_status);
    active.update(&state.db).await?;

    services::downloads::sync_album_wanted_status_from_tracks(state, album_id).await?;

    if should_enqueue {
        services::downloads::enqueue_track_download(state, track_id).await?;
    }

    info!(%track_id, %album_id, monitored, "Toggled track monitored status");
    state.notify_sse();
    Ok(())
}

pub(crate) async fn set_track_quality(
    state: &AppState,
    album_id: Uuid,
    track_id: Uuid,
    quality: Option<Quality>,
) -> AppResult<()> {
    let track = track::Entity::find_by_id(track_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("track", Some(track_id.to_string())))?;

    let mut active_track = track.into_active_model();
    active_track.quality_override = Set(quality);
    active_track.update(&state.db).await?;

    if let Some(quality) = quality {
        download_job::Entity::update_many()
            .filter(download_job::Column::AlbumId.eq(album_id))
            .filter(download_job::Column::TrackId.eq(track_id))
            .filter(
                download_job::Column::Status
                    .is_in([DownloadStatus::Queued, DownloadStatus::Failed]),
            )
            .set(download_job::ActiveModel {
                quality: Set(quality),
                ..Default::default()
            })
            .exec(&state.db)
            .await?;
    }

    info!(%album_id, %track_id, ?quality, "Updated track quality override");
    state.notify_sse();
    Ok(())
}

pub(crate) async fn bulk_toggle_track_monitor(
    state: &AppState,
    album_id: Uuid,
    monitored: bool,
) -> AppResult<()> {
    let _album = album::Entity::find_by_id(album_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("album", Some(album_id.to_string())))?;

    let next_status = if monitored {
        WantedStatus::Wanted
    } else {
        WantedStatus::Unmonitored
    };

    track::Entity::update_many()
        .set(track::ActiveModel {
            status: Set(next_status),
            ..Default::default()
        })
        .filter(track::Column::AlbumId.eq(album_id))
        .exec(&state.db)
        .await?;

    services::downloads::sync_album_wanted_status_from_tracks(state, album_id).await?;

    if monitored {
        services::downloads::enqueue_album_download(state, album_id).await?;
    }

    info!(%album_id, monitored, "Bulk toggled track monitoring");
    state.notify_sse();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sea_orm::{
        ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait,
        QueryFilter,
    };

    use super::{list_library_tracks, set_track_quality};
    use crate::{
        app_config::AuthConfig,
        db::{
            album, album_artist, album_type::AlbumType, artist, download_job,
            download_status::DownloadStatus, quality::Quality, track, wanted_status::WantedStatus,
        },
        providers::registry::ProviderRegistry,
        state::AppState,
    };

    async fn test_state() -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-track-service-test-{}.db?mode=rwc",
            uuid::Uuid::now_v7()
        );

        AppState::new(
            PathBuf::from("./music"),
            Quality::Lossless,
            false,
            1,
            &db_path,
            ProviderRegistry::new(),
            AuthConfig {
                enabled: false,
                session_secret: String::new(),
                init_admin_username: None,
                init_admin_password: None,
            },
        )
        .await
    }

    #[tokio::test]
    async fn list_library_tracks_returns_track_with_album_and_primary_artist() {
        let state = test_state().await;

        let artist = artist::ActiveModel {
            name: sea_orm::ActiveValue::Set("Test Artist".to_string()),
            image_url: sea_orm::ActiveValue::Set(None),
            bio: sea_orm::ActiveValue::Set(None),
            monitored: sea_orm::ActiveValue::Set(true),
            ..artist::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert artist");

        let album = album::ActiveModel {
            title: sea_orm::ActiveValue::Set("Test Album".to_string()),
            album_type: sea_orm::ActiveValue::Set(AlbumType::Album),
            release_date: sea_orm::ActiveValue::Set(None),
            cover_url: sea_orm::ActiveValue::Set(Some("/cover.jpg".to_string())),
            explicit: sea_orm::ActiveValue::Set(false),
            wanted_status: sea_orm::ActiveValue::Set(WantedStatus::Wanted),
            requested_quality: sea_orm::ActiveValue::Set(None),
            ..album::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album");

        album_artist::ActiveModel {
            album_id: sea_orm::ActiveValue::Set(album.id),
            artist_id: sea_orm::ActiveValue::Set(artist.id),
            priority: sea_orm::ActiveValue::Set(0),
        }
        .insert(&state.db)
        .await
        .expect("insert album artist");

        let track = track::ActiveModel {
            title: sea_orm::ActiveValue::Set("Track 1".to_string()),
            version: sea_orm::ActiveValue::Set(None),
            disc_number: sea_orm::ActiveValue::Set(Some(1)),
            track_number: sea_orm::ActiveValue::Set(Some(1)),
            duration: sea_orm::ActiveValue::Set(Some(215)),
            album_id: sea_orm::ActiveValue::Set(album.id),
            explicit: sea_orm::ActiveValue::Set(false),
            isrc: sea_orm::ActiveValue::Set(Some("ISRC123".to_string())),
            root_folder_id: sea_orm::ActiveValue::Set(None),
            status: sea_orm::ActiveValue::Set(WantedStatus::Wanted),
            quality_override: sea_orm::ActiveValue::Set(None),
            file_path: sea_orm::ActiveValue::Set(None),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert track");

        let tracks = list_library_tracks(&state)
            .await
            .expect("list library tracks");

        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].track.id, track.id);
        assert_eq!(tracks[0].album_id, album.id);
        assert_eq!(tracks[0].album_title, "Test Album");
        assert_eq!(tracks[0].album_cover_url.as_deref(), Some("/cover.jpg"));
        assert_eq!(tracks[0].artist_id, artist.id);
        assert_eq!(tracks[0].artist_name, "Test Artist");
        assert_eq!(tracks[0].track.title, "Track 1");
        assert!(tracks[0].track.monitored);
        assert!(!tracks[0].track.acquired);
    }

    #[tokio::test]
    async fn set_track_quality_persists_override_and_updates_pending_track_jobs() {
        let state = test_state().await;

        let artist = artist::ActiveModel {
            name: Set("Test Artist".to_string()),
            image_url: Set(None),
            bio: Set(None),
            monitored: Set(true),
            ..artist::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert artist");

        let album = album::ActiveModel {
            title: Set("Test Album".to_string()),
            album_type: Set(AlbumType::Album),
            release_date: Set(None),
            cover_url: Set(None),
            explicit: Set(false),
            wanted_status: Set(WantedStatus::Wanted),
            requested_quality: Set(None),
            ..album::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album");

        album_artist::ActiveModel {
            album_id: Set(album.id),
            artist_id: Set(artist.id),
            priority: Set(0),
        }
        .insert(&state.db)
        .await
        .expect("insert album artist");

        let track = track::ActiveModel {
            title: Set("Track 1".to_string()),
            version: Set(None),
            disc_number: Set(Some(1)),
            track_number: Set(Some(1)),
            duration: Set(Some(215)),
            album_id: Set(album.id),
            explicit: Set(false),
            isrc: Set(None),
            root_folder_id: Set(None),
            status: Set(WantedStatus::Wanted),
            quality_override: Set(None),
            file_path: Set(None),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert track");

        download_job::ActiveModel {
            album_id: Set(album.id),
            track_id: Set(Some(track.id)),
            source: Set(crate::db::provider::Provider::Tidal),
            quality: Set(Quality::Lossless),
            status: Set(DownloadStatus::Queued),
            total_tracks: Set(1),
            completed_tasks: Set(0),
            error_message: Set(None),
            ..download_job::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert queued job");

        download_job::ActiveModel {
            album_id: Set(album.id),
            track_id: Set(Some(track.id)),
            source: Set(crate::db::provider::Provider::Tidal),
            quality: Set(Quality::Lossless),
            status: Set(DownloadStatus::Completed),
            total_tracks: Set(1),
            completed_tasks: Set(1),
            error_message: Set(None),
            ..download_job::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert completed job");

        set_track_quality(&state, album.id, track.id, Some(Quality::HiRes))
            .await
            .expect("set quality");

        let reloaded_track = track::Entity::find_by_id(track.id)
            .one(&state.db)
            .await
            .expect("reload track")
            .expect("track exists");
        assert_eq!(reloaded_track.quality_override, Some(Quality::HiRes));

        let queued_job = download_job::Entity::find()
            .filter(download_job::Column::TrackId.eq(track.id))
            .filter(download_job::Column::Status.eq(DownloadStatus::Queued))
            .one(&state.db)
            .await
            .expect("reload queued job")
            .expect("queued job exists");
        assert_eq!(queued_job.quality, Quality::HiRes);

        let completed_job = download_job::Entity::find()
            .filter(download_job::Column::TrackId.eq(track.id))
            .filter(download_job::Column::Status.eq(DownloadStatus::Completed))
            .one(&state.db)
            .await
            .expect("reload completed job")
            .expect("completed job exists");
        assert_eq!(completed_job.quality, Quality::Lossless);

        set_track_quality(&state, album.id, track.id, None)
            .await
            .expect("clear quality");

        let cleared_track = track::Entity::find_by_id(track.id)
            .one(&state.db)
            .await
            .expect("reload cleared track")
            .expect("track exists");
        assert_eq!(cleared_track.quality_override, None);
    }
}
