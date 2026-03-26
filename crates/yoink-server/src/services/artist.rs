use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, PaginatorTrait};
use tracing::info;
use url::Url;
use uuid::Uuid;

use crate::{
    db::{self, album_artist, artist, artist_provider_link},
    error::{AppError, AppResult},
    state::AppState,
};

use super::helpers;

pub(crate) async fn add_artist(
    state: &AppState,
    name: String,
    provider: String,
    external_id: String,
    image_url: Option<String>,
    external_url: Option<Url>,
) -> AppResult<()> {
    let provider_enum = helpers::parse_provider(&provider)?;
    let external_url = external_url
        .map(|u| u.to_string())
        .or_else(|| helpers::default_provider_artist_url(&provider, &external_id));
    let external_name = name.clone();

    let artist_id = helpers::find_or_create_artist_with_provider_link(
        state,
        provider_enum,
        &external_id,
        &name,
        image_url,
        true,
        external_url,
        Some(external_name),
    )
    .await?;

    super::sync_artist(state, artist_id).await?;
    helpers::spawn_fetch_artist_bio(state, artist_id);
    helpers::spawn_recompute_artist_match_suggestions(state, artist_id);
    state.notify_sse();
    Ok(())
}

pub(crate) async fn remove_artist(
    state: &AppState,
    artist_id: Uuid,
    remove_files: bool,
) -> AppResult<()> {
    // Delete albums solely owned by this artist.
    // Multi-artist albums are kept; the cascade will remove the junction row.
    let album_artists = album_artist::Entity::find_by_artist(artist_id)
        .all(&state.db)
        .await?;

    for aa in &album_artists {
        let artist_count = album_artist::Entity::find_by_album_ordered(aa.album_id)
            .count(&state.db)
            .await?;

        if artist_count <= 1 {
            if remove_files {
                // TODO: remove downloaded files for this album
            }
            db::album::Entity::delete_by_id(aa.album_id)
                .exec(&state.db)
                .await?;
        }
    }

    // Cascade deletes provider links, match suggestions, album junctions,
    // and track-artist junctions.
    artist::Entity::delete_by_id(artist_id)
        .exec(&state.db)
        .await?;

    info!(%artist_id, remove_files, "Removed artist");
    state.notify_sse();
    Ok(())
}

pub(crate) async fn update_artist(
    state: &AppState,
    artist_id: Uuid,
    name: Option<String>,
    image_url: Option<String>,
) -> AppResult<()> {
    let mut model: artist::ActiveModel = artist::Entity::find_by_id(artist_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("artist", Some(artist_id.to_string())))?
        .into();

    if let Some(ref name) = name {
        model.name = Set(name.clone());
    }
    if let Some(image_url) = image_url {
        model.image_url = Set(Some(image_url));
    }

    model.update(&state.db).await?;

    info!(%artist_id, ?name, "Updated artist details");
    state.notify_sse();
    Ok(())
}

pub(crate) async fn toggle_artist_monitor(
    state: &AppState,
    artist_id: Uuid,
    monitored: bool,
) -> AppResult<()> {
    let mut model: artist::ActiveModel = artist::Entity::find_by_id(artist_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("artist", Some(artist_id.to_string())))?
        .into();

    model.monitored = Set(monitored);
    model.update(&state.db).await?;

    if monitored {
        super::sync_artist(state, artist_id).await?;
        helpers::spawn_fetch_artist_bio(state, artist_id);
        helpers::spawn_recompute_artist_match_suggestions(state, artist_id);
    }
    info!(%artist_id, monitored, "Toggled artist monitored status");
    state.notify_sse();
    Ok(())
}

pub(crate) async fn fetch_artist_bio(state: &AppState, artist_id: Uuid) -> AppResult<()> {
    info!(%artist_id, "Manual bio fetch requested, clearing existing bio");

    let mut model: artist::ActiveModel = artist::Entity::find_by_id(artist_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("artist", Some(artist_id.to_string())))?
        .into();

    model.bio = Set(None);
    model.update(&state.db).await?;

    state.notify_sse();
    helpers::spawn_fetch_artist_bio(state, artist_id);
    Ok(())
}

pub(crate) async fn sync_artist_and_refresh(state: &AppState, artist_id: Uuid) -> AppResult<()> {
    super::sync_artist(state, artist_id).await?;

    let artist = artist::Entity::find_by_id(artist_id).one(&state.db).await?;
    let has_bio = artist.as_ref().and_then(|a| a.bio.as_ref()).is_some();

    if !has_bio {
        helpers::spawn_fetch_artist_bio(state, artist_id);
    }

    helpers::spawn_recompute_artist_match_suggestions(state, artist_id);
    state.notify_sse();
    Ok(())
}

pub(crate) async fn link_artist_provider(
    state: &AppState,
    artist_id: Uuid,
    provider: String,
    external_id: String,
    external_url: Option<String>,
    external_name: Option<String>,
    _image_ref: Option<String>,
) -> AppResult<()> {
    let provider_enum = helpers::parse_provider(&provider)?;
    let external_url =
        external_url.or_else(|| helpers::default_provider_artist_url(&provider, &external_id));

    helpers::upsert_artist_provider_link(
        state,
        artist_id,
        provider_enum,
        &external_id,
        external_url,
        external_name,
    )
    .await?;

    let artist = artist::Entity::find_by_id(artist_id).one(&state.db).await?;
    let has_bio = artist.as_ref().and_then(|a| a.bio.as_ref()).is_some();
    if !has_bio {
        helpers::spawn_fetch_artist_bio(state, artist_id);
    }

    helpers::spawn_recompute_artist_match_suggestions(state, artist_id);
    state.notify_sse();
    Ok(())
}

pub(crate) async fn unlink_artist_provider(
    state: &AppState,
    artist_id: Uuid,
    provider: String,
    external_id: String,
) -> AppResult<()> {
    let provider_enum = helpers::parse_provider(&provider)?;

    artist_provider_link::Entity::delete_by_artist_provider_external(
        artist_id,
        provider_enum,
        &external_id,
    )
    .exec(&state.db)
    .await?;

    helpers::spawn_recompute_artist_match_suggestions(state, artist_id);
    state.notify_sse();
    Ok(())
}
