use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
};
use tracing::info;
use uuid::Uuid;

use crate::{
    db::{
        album, provider::Provider, quality::Quality, track, track_provider_link,
        wanted_status::WantedStatus,
    },
    error::{AppError, AppResult},
    services,
    state::AppState,
};

use super::helpers;

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

    if let Some(link) = target_link {
        if let Some(found) = track::Entity::find_by_id(link.track_id)
            .one(&state.db)
            .await?
        {
            let mut model: track::ActiveModel = found.into();
            model.status = Set(WantedStatus::Wanted);
            model.update(&state.db).await?;
            services::downloads::enqueue_track_download(state, link.track_id).await?;
        }
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
    let _track = track::Entity::find_by_id(track_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("track", Some(track_id.to_string())))?;

    // TODO: set track.quality_override once the column exists on the entity

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
