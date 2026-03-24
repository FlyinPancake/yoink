use sea_orm::{
    ActiveEnum, ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait,
    QueryFilter,
};
use tracing::info;
use uuid::Uuid;

use crate::{
    db::{
        album, album_artist, album_provider_link, album_type::AlbumType, provider::Provider,
        quality::Quality, track, url::DbUrl, wanted_status::WantedStatus,
    },
    error::{AppError, AppResult},
    providers::provider_image_url,
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
            .and_then(|t| AlbumType::try_from_value(&t.to_string()).ok())
            .unwrap_or(AlbumType::Album);

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

    // 4. Fetch and store tracks (none monitored by default).
    helpers::store_album_tracks(state, provider, &external_album_id, album_id, false).await?;

    // 5. Find the target track and mark it as monitored.
    // TODO: once track has a provider_link relation via SeaORM, look up and set monitored
    // For now the track is stored but not individually marked.

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
    let _track = track::Entity::find_by_id(track_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("track", Some(track_id.to_string())))?;

    // TODO: set track.monitored once the column exists on the entity
    // TODO: recompute album wanted_status based on track states
    // TODO: enqueue download if track became wanted

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

    // TODO: update all tracks for album once track has album_id + monitored columns
    // TODO: recompute album wanted_status
    // TODO: enqueue download if wanted

    info!(%album_id, monitored, "Bulk toggled track monitoring");
    state.notify_sse();
    Ok(())
}
