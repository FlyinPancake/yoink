use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    ColumnTrait, DatabaseConnection, EntityTrait, ModelTrait, QueryFilter,
};
use serde::Serialize;
use tracing::info;
use utoipa::ToSchema;
use uuid::Uuid;
use yoink_shared::{Album, DownloadJob, MonitoredArtist, ProviderLink, TrackInfo};

use crate::{
    db::{
        self, album, album_artist, album_provider_link, album_type::AlbumType, download_job,
        download_status::DownloadStatus, provider::Provider, quality::Quality,
        wanted_status::WantedStatus,
    },
    error::{AppError, AppResult},
    providers::provider_image_url,
    services,
    state::AppState,
};

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

    let jobs = album
        .find_related(db::download_job::Entity)
        .all(&state.db)
        .await?
        .into_iter()
        .map(|j| j.into_ex().into())
        .collect();

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

    // TODO: enqueue download if wanted

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

pub(crate) async fn bulk_monitor(
    state: &AppState,
    artist_id: Uuid,
    monitored: bool,
) -> AppResult<()> {
    let album_ids: Vec<Uuid> = album_artist::Entity::find_by_artist(artist_id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(|aa| aa.album_id)
        .collect();

    for album_id in album_ids {
        toggle_album_monitor(state, album_id, monitored).await?;
    }

    Ok(())
}

pub(crate) async fn set_album_quality(
    state: &AppState,
    album_id: Uuid,
    quality: Option<Quality>,
) -> AppResult<()> {
    album::Entity::find_by_id(album_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("album", Some(album_id.to_string())))?;

    album::Entity::update_many()
        .set(album::ActiveModel {
            id: NotSet,
            requested_quality: Set(quality),
            ..Default::default()
        })
        .filter(album::Column::Id.eq(album_id))
        .exec(&state.db)
        .await?;

    // Update quality on queued/failed download jobs for this album
    if let Some(quality) = quality {
        let jobs = download_job::Entity::find()
            .filter(download_job::Column::AlbumId.eq(album_id))
            .filter(
                download_job::Column::Status
                    .is_in([DownloadStatus::Queued, DownloadStatus::Failed]),
            )
            .all(&state.db)
            .await?;

        for job in jobs {
            let mut job_model: download_job::ActiveModel = job.into();
            job_model.quality = Set(quality);
            job_model.update(&state.db).await?;
        }
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
    // 1. Find or create lightweight (unmonitored) artist.
    let artist_id = helpers::find_or_create_lightweight_artist(
        state,
        provider,
        &artist_external_id,
        &artist_name,
    )
    .await?;

    // 2. Fetch album metadata from the provider.
    let metadata_provider = state.registry.metadata_provider(provider).ok_or_else(|| {
        AppError::unavailable(
            "metadata provider",
            format!("unknown provider '{provider}'"),
        )
    })?;

    let albums = metadata_provider.fetch_albums(&artist_external_id).await?;

    let prov_album = albums
        .into_iter()
        .find(|a| a.external_id == external_album_id)
        .ok_or_else(|| {
            AppError::not_found(
                "provider album",
                Some(format!("{provider}:{external_album_id}")),
            )
        })?;

    let existing = album_provider_link::Entity::find()
        .filter(album_provider_link::Column::Provider.eq(provider))
        .filter(album_provider_link::Column::ProviderAlbumId.eq(&external_album_id))
        .one(&state.db)
        .await?;

    let album_id = if let Some(link) = existing {
        link.album_id
    } else {
        let album_type = prov_album
            .album_type
            .as_deref()
            .map(AlbumType::parse)
            .unwrap_or(AlbumType::Unknown);

        let release_date = prov_album.release_date;

        let cover_url = prov_album
            .cover_ref
            .as_ref()
            .map(|r| provider_image_url(provider, r, 640));

        let wanted_status = if monitor_all {
            WantedStatus::Wanted
        } else {
            WantedStatus::Unmonitored
        };

        let model = album::ActiveModel {
            title: Set(prov_album.title.clone()),
            album_type: Set(album_type),
            release_date: Set(release_date),
            cover_url: Set(cover_url),
            explicit: Set(prov_album.explicit),
            wanted_status: Set(wanted_status),
            ..album::ActiveModel::new()
        };
        let new_album = model.insert(&state.db).await?;
        let new_id = new_album.id;

        // Create provider link
        let link = album_provider_link::ActiveModel {
            album_id: Set(new_id),
            provider: Set(provider),
            provider_album_id: Set(external_album_id.clone()),
            external_url: Set(prov_album.url.clone()),
            external_name: Set(Some(prov_album.title.clone())),
            ..album_provider_link::ActiveModel::new()
        };
        link.insert(&state.db).await?;

        // Create junction table entry
        let junction = album_artist::ActiveModel {
            album_id: Set(new_id),
            artist_id: Set(artist_id),
            priority: Set(0),
        };
        junction.insert(&state.db).await?;

        new_id
    };

    // 4. Sync tracks from provider.
    super::sync_album_tracks(state, provider, &external_album_id, album_id).await?;

    // TODO: enqueue download via SeaORM once download service is migrated

    info!(%album_id, %provider, %external_album_id, monitor_all, "Added album from search");
    state.notify_sse();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sea_orm::{ActiveModelBehavior, ActiveModelTrait, EntityTrait};

    use super::toggle_album_monitor;
    use crate::{
        app_config::AuthConfig,
        db::{
            self, album, album_type::AlbumType, quality::Quality, track,
            wanted_status::WantedStatus,
        },
        providers::registry::ProviderRegistry,
        state::AppState,
    };

    async fn test_state() -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-album-monitor-test-{}.db?mode=rwc",
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
    async fn toggle_album_monitor_updates_album_and_track_statuses() {
        let state = test_state().await;

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
}
