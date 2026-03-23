use sea_orm::{ActiveEnum, ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, EntityTrait};
use tracing::info;
use url::Url;
use uuid::Uuid;

use crate::{
    db::{self, album_artist, artist, artist_provider_link, provider::Provider, url::DbUrl},
    error::{AppError, AppResult},
    state::AppState,
};

use super::helpers;

/// Parse a provider string into the `Provider` enum.
fn parse_provider(s: &str) -> AppResult<Provider> {
    Provider::try_from_value(&s.to_string()).map_err(|_| AppError::Validation {
        field: Some("provider".into()),
        reason: format!("unknown provider '{s}'"),
    })
}

pub(crate) async fn add_artist(
    state: &AppState,
    name: String,
    provider: String,
    external_id: String,
    image_url: Option<Url>,
    external_url: Option<Url>,
) -> AppResult<()> {
    let provider_enum = parse_provider(&provider)?;
    let external_url = external_url
        .map(|u| u.to_string())
        .or_else(|| helpers::default_provider_artist_url(&provider, &external_id));

    // Check if artist already exists via provider link
    let existing =
        artist_provider_link::Entity::find_by_provider_external(provider_enum, &external_id)
            .one(&state.db)
            .await?;

    let artist_id = if let Some(link) = existing {
        link.artist_id
    } else {
        let model = artist::ActiveModel {
            name: Set(name.clone()),
            image_url: Set(image_url.map(DbUrl)),
            monitored: Set(true),
            ..artist::ActiveModel::new()
        };
        let artist = model.insert(&state.db).await?;
        let artist_id = artist.id;

        let link = artist_provider_link::ActiveModel {
            artist_id: Set(artist_id),
            provider: Set(provider_enum),
            external_id: Set(external_id),
            external_url: Set(external_url),
            external_name: Set(Some(name)),
            ..artist_provider_link::ActiveModel::new()
        };
        link.insert(&state.db).await?;

        artist_id
    };

    super::sync_artist_albums(state, artist_id).await?;
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
    if remove_files {
        let album_artists = album_artist::Entity::find_by_artist(artist_id)
            .all(&state.db)
            .await?;

        for aa in &album_artists {
            if let Some(album) = db::album::Entity::find_by_id(aa.album_id)
                .one(&state.db)
                .await?
            {
                // TODO: check acquired status and remove files
                let _ = album;
            }
        }
    }

    // Remove albums solely owned by this artist; for multi-artist albums
    // just detach this artist.
    let album_artists = album_artist::Entity::find_by_artist(artist_id)
        .all(&state.db)
        .await?;

    for aa in &album_artists {
        let all_artists_for_album = album_artist::Entity::find_by_album_ordered(aa.album_id)
            .all(&state.db)
            .await?;

        if all_artists_for_album.len() <= 1 {
            db::album::Entity::delete_by_id(aa.album_id)
                .exec(&state.db)
                .await?;
        } else {
            album_artist::Entity::delete_pair(aa.album_id, artist_id)
                .exec(&state.db)
                .await?;
        }
    }

    artist::Entity::delete_by_id(artist_id)
        .exec(&state.db)
        .await?;

    info!(%artist_id, remove_files, "Removed artist and their albums");
    state.notify_sse();
    Ok(())
}

pub(crate) async fn update_artist(
    state: &AppState,
    artist_id: Uuid,
    name: Option<String>,
    image_url: Option<Url>,
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
        model.image_url = Set(Some(DbUrl(image_url)));
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
        super::sync_artist_albums(state, artist_id).await?;
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

pub(crate) async fn sync_artist_albums(state: &AppState, artist_id: Uuid) -> AppResult<()> {
    super::sync_artist_albums(state, artist_id).await?;

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
    let provider_enum = parse_provider(&provider)?;
    let external_url =
        external_url.or_else(|| helpers::default_provider_artist_url(&provider, &external_id));

    let existing = artist_provider_link::Entity::find_by_artist_provider_external(
        artist_id,
        provider_enum,
        &external_id,
    )
    .one(&state.db)
    .await?;

    if let Some(existing) = existing {
        let mut model: artist_provider_link::ActiveModel = existing.into();
        model.external_url = Set(external_url);
        model.external_name = Set(external_name);
        model.update(&state.db).await?;
    } else {
        let model = artist_provider_link::ActiveModel {
            artist_id: Set(artist_id),
            provider: Set(provider_enum),
            external_id: Set(external_id),
            external_url: Set(external_url),
            external_name: Set(external_name),
            ..artist_provider_link::ActiveModel::new()
        };
        model.insert(&state.db).await?;
    }

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
    let provider_enum = parse_provider(&provider)?;

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
