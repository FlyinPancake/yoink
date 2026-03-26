use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait,
    IntoActiveModel, QueryFilter,
};
use tracing::info;
use uuid::Uuid;

use crate::{
    db::{
        album, album_artist, album_provider_link, album_type::AlbumType, provider::Provider,
        quality::Quality, track, track_provider_link, wanted_status::WantedStatus,
    },
    error::{AppError, AppResult},
    providers::provider_image_url,
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
    // 1. Find or create lightweight (unmonitored) artist.
    let artist_id = helpers::find_or_create_lightweight_artist(
        state,
        provider,
        &artist_external_id,
        &artist_name,
    )
    .await?;

    // 2. Fetch album metadata to create the parent album.
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

        // Album-level not monitored; only the specific track will be
        let model = album::ActiveModel {
            title: Set(prov_album.title.clone()),
            album_type: Set(album_type),
            release_date: Set(release_date),
            cover_url: Set(cover_url),
            explicit: Set(prov_album.explicit),
            wanted_status: Set(WantedStatus::Unmonitored), // has a wanted track
            ..album::ActiveModel::new()
        };
        let new_album = model.insert(&state.db).await?;
        let new_id = new_album.id;

        let link = album_provider_link::ActiveModel {
            album_id: Set(new_id),
            provider: Set(provider),
            provider_album_id: Set(external_album_id.clone()),
            external_url: Set(prov_album.url.clone()),
            external_name: Set(Some(prov_album.title.clone())),
            ..album_provider_link::ActiveModel::new()
        };
        link.insert(&state.db).await?;

        let junction = album_artist::ActiveModel {
            album_id: Set(new_id),
            artist_id: Set(artist_id),
            priority: Set(0),
            ..Default::default()
        };
        junction.insert(&state.db).await?;

        new_id
    };

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
