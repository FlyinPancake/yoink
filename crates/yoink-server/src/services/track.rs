use std::collections::{HashMap, HashSet};

use crate::api::LibraryTrack;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, SelectExt,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    db::{
        self, album, provider::Provider, quality::Quality, track, track_provider_link,
        wanted_status::WantedStatus,
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
        services::jobs::download::enqueue_download_track_job(state, link.track_id).await?;
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
    if !monitored {
        services::jobs::prepare_track_for_unmonitor(state, track_id).await?;
    }

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
        services::jobs::download::enqueue_download_track_job(state, track_id).await?;
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

    info!(%album_id, %track_id, ?quality, "Updated track quality override");
    state.notify_sse();
    Ok(())
}

pub(crate) async fn bulk_toggle_track_monitor(
    state: &AppState,
    album_id: Uuid,
    monitored: bool,
) -> AppResult<()> {
    if !album::Entity::find_by_id(album_id)
        .exists(&state.db)
        .await?
    {
        return Err(AppError::not_found("album", Some(album_id.to_string())));
    }

    let next_status = if monitored {
        WantedStatus::Wanted
    } else {
        WantedStatus::Unmonitored
    };

    let tracks = track::Entity::find()
        .filter(track::Column::AlbumId.eq(album_id))
        .all(&state.db)
        .await?;

    // TODO maybe make this better
    if !monitored {
        for track in tracks {
            services::jobs::prepare_track_for_unmonitor(state, track.id).await?;
        }
    }

    track::Entity::update_many()
        .set(track::ActiveModel {
            status: Set(next_status),
            ..Default::default()
        })
        .filter(track::Column::AlbumId.eq(album_id))
        .exec(&state.db)
        .await?;

    services::downloads::sync_album_wanted_status_from_tracks(state, album_id).await?;

    info!(%album_id, monitored, "Bulk toggled track monitoring");
    state.notify_sse();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sea_orm::{ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, EntityTrait};

    use super::{list_library_tracks, set_track_quality, toggle_track_monitor};
    use crate::{
        db::{
            album, album_artist, album_type::AlbumType, artist, job_status::JobStatus,
            provider::Provider, quality::Quality, track, wanted_status::WantedStatus,
        },
        error::AppError,
        providers::{mock::MockProvider, registry::ProviderRegistry},
        services::jobs::{self, Job, download::DownloadTrackJobPayload},
        test_support,
    };

    #[tokio::test]
    async fn list_library_tracks_returns_track_with_album_and_primary_artist() {
        let state = test_support::test_state().await;

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
    async fn set_track_quality_persists_and_clears_override() {
        let state = test_support::test_state().await;

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

        set_track_quality(&state, album.id, track.id, Some(Quality::HiRes))
            .await
            .expect("set quality");

        let reloaded_track = track::Entity::find_by_id(track.id)
            .one(&state.db)
            .await
            .expect("reload track")
            .expect("track exists");
        assert_eq!(reloaded_track.quality_override, Some(Quality::HiRes));

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

    #[tokio::test]
    async fn toggle_track_monitor_unmonitor_cancels_queued_track_jobs_and_remonitor_requeues() {
        let mut registry = ProviderRegistry::new();
        registry.register_download(crate::providers::DownloadSource::Search(Arc::new(
            MockProvider {},
        )));

        let state = test_support::test_state_with_registry(registry).await;

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

        let job = jobs::enqueue_job(
            &state,
            Job::DownloadTrack {
                payload: DownloadTrackJobPayload {
                    track_id: track.id,
                    provider: Provider::Tidal,
                },
            },
        )
        .await
        .expect("insert queued job");

        toggle_track_monitor(&state, track.id, album.id, false)
            .await
            .expect("unmonitor track");

        let reloaded_track = track::Entity::find_by_id(track.id)
            .one(&state.db)
            .await
            .expect("reload track")
            .expect("track exists");
        let reloaded_job = crate::db::job::Entity::find_by_id(job.id)
            .one(&state.db)
            .await
            .expect("reload job")
            .expect("job exists");

        assert_eq!(reloaded_track.status, WantedStatus::Unmonitored);
        assert_eq!(reloaded_job.status, JobStatus::Cancelled);

        toggle_track_monitor(&state, track.id, album.id, false)
            .await
            .expect("unmonitor already unmonitored track");

        let reloaded_job = crate::db::job::Entity::find_by_id(job.id)
            .one(&state.db)
            .await
            .expect("reload already cancelled job")
            .expect("job exists");

        assert_eq!(reloaded_job.status, JobStatus::Cancelled);

        toggle_track_monitor(&state, track.id, album.id, true)
            .await
            .expect("remonitor track");

        let reloaded_track = track::Entity::find_by_id(track.id)
            .one(&state.db)
            .await
            .expect("reload remonitored track")
            .expect("track exists");
        let reloaded_job = crate::db::job::Entity::find_by_id(job.id)
            .one(&state.db)
            .await
            .expect("reload remonitored job")
            .expect("job exists");

        assert_eq!(reloaded_track.status, WantedStatus::Wanted);
        assert_eq!(reloaded_job.status, JobStatus::Queued);
    }

    #[tokio::test]
    async fn toggle_track_monitor_unmonitor_conflicts_when_track_job_running() {
        let state = test_support::test_state().await;

        let album = album::ActiveModel {
            title: Set("Busy Album".to_string()),
            album_type: Set(AlbumType::Album),
            release_date: Set(None),
            cover_url: Set(None),
            explicit: Set(false),
            wanted_status: Set(WantedStatus::InProgress),
            requested_quality: Set(None),
            ..album::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album");

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

        let job = jobs::enqueue_job(
            &state,
            Job::DownloadTrack {
                payload: DownloadTrackJobPayload {
                    track_id: track.id,
                    provider: Provider::Tidal,
                },
            },
        )
        .await
        .expect("insert queued job");
        crate::db::job::ActiveModel {
            id: Set(job.id),
            status: Set(JobStatus::Running),
            ..Default::default()
        }
        .update(&state.db)
        .await
        .expect("mark job running");

        let err = toggle_track_monitor(&state, track.id, album.id, false)
            .await
            .expect_err("active download should conflict");

        assert!(matches!(err, AppError::Conflict { .. }));
    }
}
