use sea_orm::{
    ActiveEnum, ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait,
    QueryFilter,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    db::{
        album, album_artist, album_provider_link, album_type::AlbumType, artist,
        artist_provider_link, provider::Provider, wanted_status::WantedStatus,
    },
    error::{AppError, AppResult},
    providers::{ProviderAlbum, provider_image_url},
    state::AppState,
};

/// Generate a default external URL for an artist on a given provider.
pub(crate) fn default_provider_artist_url(provider: &str, external_id: &str) -> Option<String> {
    match provider {
        "tidal" => Some(format!("https://tidal.com/browse/artist/{external_id}")),
        "deezer" => Some(format!("https://www.deezer.com/artist/{external_id}")),
        "musicbrainz" => Some(format!("https://musicbrainz.org/artist/{external_id}")),
        _ => None,
    }
}

/// Generate a default external URL for an album on a given provider.
pub(crate) fn default_provider_album_url(provider: &str, external_id: &str) -> Option<String> {
    match provider {
        "tidal" => Some(format!("https://tidal.com/browse/album/{external_id}")),
        "deezer" => Some(format!("https://www.deezer.com/album/{external_id}")),
        "musicbrainz" => Some(format!(
            "https://musicbrainz.org/release-group/{external_id}"
        )),
        _ => None,
    }
}

pub(crate) fn parse_provider(s: &str) -> AppResult<Provider> {
    Provider::try_from_value(&s.to_string()).map_err(|_| AppError::Validation {
        field: Some("provider".into()),
        reason: format!("unknown provider '{s}'"),
    })
}

/// Fetch artist bio from linked metadata providers in background.
pub(crate) fn spawn_fetch_artist_bio(state: &AppState, artist_id: Uuid) {
    let s = state.clone();
    info!(%artist_id, "Spawning background bio fetch");
    tokio::spawn(async move {
        let artist = match artist::Entity::find_by_id(artist_id).one(&s.db).await {
            Ok(Some(artist)) => artist,
            Ok(None) => {
                info!(%artist_id, "Artist disappeared before bio fetch started");
                return;
            }
            Err(err) => {
                warn!(%artist_id, error = %err, "Failed to load artist for bio fetch");
                return;
            }
        };

        let links = match artist_provider_link::Entity::find_by_artist(artist_id)
            .all(&s.db)
            .await
        {
            Ok(links) => links,
            Err(err) => {
                warn!(%artist_id, error = %err, "Failed to load provider links for bio fetch");
                return;
            }
        };

        if links.is_empty() {
            info!(%artist_id, "No provider links found, skipping bio fetch");
            return;
        }

        info!(%artist_id, link_count = links.len(), "Attempting bio fetch from linked providers");

        for link in links {
            let Some(provider) = s.registry.metadata_provider(link.provider) else {
                info!(%artist_id, provider = %link.provider, "Provider not available as metadata source, skipping");
                continue;
            };

            info!(
                %artist_id,
                provider = %link.provider,
                external_id = %link.external_id,
                "Fetching bio from provider"
            );

            match provider.fetch_artist_bio(&link.external_id).await {
                Some(bio) if !bio.trim().is_empty() => {
                    let bio_len = bio.len();
                    let mut model: artist::ActiveModel = artist.clone().into();
                    model.bio = Set(Some(bio));

                    if let Err(err) = model.update(&s.db).await {
                        warn!(
                            %artist_id,
                            provider = %link.provider,
                            error = %err,
                            "Failed to persist fetched bio"
                        );
                        return;
                    }

                    info!(
                        %artist_id,
                        provider = %link.provider,
                        bio_len,
                        "Successfully fetched and saved artist bio"
                    );
                    s.notify_sse();
                    return;
                }
                Some(_) | None => {
                    info!(
                        %artist_id,
                        provider = %link.provider,
                        external_id = %link.external_id,
                        "Provider returned no bio"
                    );
                }
            }
        }

        info!(%artist_id, "No provider returned a bio for this artist");
    });
}

pub(crate) fn spawn_recompute_artist_match_suggestions(state: &AppState, artist_id: Uuid) {
    let s = state.clone();
    tokio::spawn(async move {
        if let Err(err) = super::recompute_artist_match_suggestions(&s, artist_id).await {
            warn!(artist_id = %artist_id, error = %err, "Background match recompute failed");
        }
        s.notify_sse();
    });
}

pub(crate) async fn upsert_artist_provider_link(
    state: &AppState,
    artist_id: Uuid,
    provider: Provider,
    external_id: &str,
    external_url: Option<String>,
    external_name: Option<String>,
) -> AppResult<()> {
    let existing = artist_provider_link::Entity::find_by_artist_provider_external(
        artist_id,
        provider,
        external_id,
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
            provider: Set(provider),
            external_id: Set(external_id.to_string()),
            external_url: Set(external_url),
            external_name: Set(external_name),
            ..artist_provider_link::ActiveModel::new()
        };
        model.insert(&state.db).await?;
    }

    Ok(())
}

pub(crate) async fn find_or_create_artist_with_provider_link(
    state: &AppState,
    provider: Provider,
    external_id: &str,
    name: &str,
    image_url: Option<String>,
    monitored: bool,
    external_url: Option<String>,
    external_name: Option<String>,
) -> AppResult<Uuid> {
    if let Some(existing) =
        artist_provider_link::Entity::find_by_provider_external(provider, external_id)
            .one(&state.db)
            .await?
    {
        return Ok(existing.artist_id);
    }

    let artist = artist::ActiveModel {
        name: Set(name.to_string()),
        image_url: Set(image_url),
        bio: Set(None),
        monitored: Set(monitored),
        ..artist::ActiveModel::new()
    }
    .insert(&state.db)
    .await?;

    upsert_artist_provider_link(
        state,
        artist.id,
        provider,
        external_id,
        external_url,
        external_name,
    )
    .await?;

    Ok(artist.id)
}

async fn fetch_provider_album(
    state: &AppState,
    provider: Provider,
    artist_external_id: &str,
    external_album_id: &str,
) -> AppResult<ProviderAlbum> {
    let metadata_provider = state.registry.metadata_provider(provider).ok_or_else(|| {
        AppError::unavailable(
            "metadata provider",
            format!("unknown provider '{provider}'"),
        )
    })?;

    metadata_provider
        .fetch_albums(artist_external_id)
        .await?
        .into_iter()
        .find(|album| album.external_id == external_album_id)
        .ok_or_else(|| {
            AppError::not_found(
                "provider album",
                Some(format!("{provider}:{external_album_id}")),
            )
        })
}

pub(crate) async fn ensure_local_album(
    state: &AppState,
    provider: Provider,
    external_album_id: &str,
    artist_external_id: &str,
    artist_name: &str,
    wanted_status: WantedStatus,
) -> AppResult<Uuid> {
    let artist_id =
        find_or_create_lightweight_artist(state, provider, artist_external_id, artist_name).await?;

    if let Some(link) = album_provider_link::Entity::find()
        .filter(album_provider_link::Column::Provider.eq(provider))
        .filter(album_provider_link::Column::ProviderAlbumId.eq(external_album_id))
        .one(&state.db)
        .await?
    {
        return Ok(link.album_id);
    }

    let provider_album =
        fetch_provider_album(state, provider, artist_external_id, external_album_id).await?;

    let album_type = provider_album
        .album_type
        .as_deref()
        .map(AlbumType::parse)
        .unwrap_or(AlbumType::Unknown);
    let cover_url = provider_album
        .cover_ref
        .as_ref()
        .map(|image_ref| provider_image_url(provider, image_ref, 640));

    let created_album = album::ActiveModel {
        title: Set(provider_album.title.clone()),
        album_type: Set(album_type),
        release_date: Set(provider_album.release_date),
        cover_url: Set(cover_url),
        explicit: Set(provider_album.explicit),
        wanted_status: Set(wanted_status),
        ..album::ActiveModel::new()
    }
    .insert(&state.db)
    .await?;

    let album_id = created_album.id;

    let link = album_provider_link::ActiveModel {
        album_id: Set(album_id),
        provider: Set(provider),
        provider_album_id: Set(external_album_id.to_string()),
        external_url: Set(provider_album.url),
        external_name: Set(Some(provider_album.title)),
        ..album_provider_link::ActiveModel::new()
    };
    link.insert(&state.db).await?;

    let junction = album_artist::ActiveModel {
        album_id: Set(album_id),
        artist_id: Set(artist_id),
        priority: Set(0),
        ..Default::default()
    };
    junction.insert(&state.db).await?;

    Ok(album_id)
}

/// Find an existing artist by provider link, or create a new lightweight
/// (unmonitored) artist with a single provider link.
pub(crate) async fn find_or_create_lightweight_artist(
    state: &AppState,
    provider: Provider,
    artist_external_id: &str,
    artist_name: &str,
) -> AppResult<Uuid> {
    let external_url = default_provider_artist_url(&provider.to_string(), artist_external_id);
    find_or_create_artist_with_provider_link(
        state,
        provider,
        artist_external_id,
        artist_name,
        None,
        false,
        external_url,
        Some(artist_name.to_string()),
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sea_orm::EntityTrait;

    use crate::{app_config::AuthConfig, providers::registry::ProviderRegistry, state::AppState};

    async fn test_state() -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-helpers-test-{}.db?mode=rwc",
            uuid::Uuid::now_v7()
        );

        AppState::new(
            PathBuf::from("./music"),
            crate::db::quality::Quality::Lossless,
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

    #[test]
    fn default_provider_artist_url_known_providers() {
        assert_eq!(
            super::default_provider_artist_url("tidal", "123"),
            Some("https://tidal.com/browse/artist/123".to_string())
        );
        assert_eq!(
            super::default_provider_artist_url("deezer", "456"),
            Some("https://www.deezer.com/artist/456".to_string())
        );
        assert_eq!(
            super::default_provider_artist_url("musicbrainz", "abc"),
            Some("https://musicbrainz.org/artist/abc".to_string())
        );
        assert!(super::default_provider_artist_url("unknown", "x").is_none());
    }

    #[test]
    fn default_provider_album_url_known_providers() {
        assert_eq!(
            super::default_provider_album_url("tidal", "123"),
            Some("https://tidal.com/browse/album/123".to_string())
        );
        assert_eq!(
            super::default_provider_album_url("deezer", "456"),
            Some("https://www.deezer.com/album/456".to_string())
        );
        assert_eq!(
            super::default_provider_album_url("musicbrainz", "abc"),
            Some("https://musicbrainz.org/release-group/abc".to_string())
        );
        assert!(super::default_provider_album_url("unknown", "x").is_none());
    }

    #[tokio::test]
    async fn find_or_create_lightweight_artist_persists_artist_and_link() {
        let state = test_state().await;

        let artist_id = super::find_or_create_lightweight_artist(
            &state,
            crate::db::provider::Provider::Tidal,
            "artist-123",
            "Test Artist",
        )
        .await
        .expect("create lightweight artist");

        let artist = crate::db::artist::Entity::find_by_id(artist_id)
            .one(&state.db)
            .await
            .expect("load artist")
            .expect("artist exists");
        let link = crate::db::artist_provider_link::Entity::find_by_provider_external(
            crate::db::provider::Provider::Tidal,
            "artist-123",
        )
        .one(&state.db)
        .await
        .expect("load provider link")
        .expect("link exists");

        assert_eq!(artist.name, "Test Artist");
        assert!(!artist.monitored);
        assert_eq!(link.artist_id, artist_id);
        assert_eq!(link.external_name.as_deref(), Some("Test Artist"));

        let same_artist_id = super::find_or_create_lightweight_artist(
            &state,
            crate::db::provider::Provider::Tidal,
            "artist-123",
            "Ignored Name",
        )
        .await
        .expect("reuse existing lightweight artist");

        assert_eq!(same_artist_id, artist_id);
    }
}
