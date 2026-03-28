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
pub(crate) fn default_provider_artist_url(provider: Provider, external_id: &str) -> Option<String> {
    match provider {
        Provider::Tidal => Some(format!("https://tidal.com/browse/artist/{external_id}")),
        Provider::Deezer => Some(format!("https://www.deezer.com/artist/{external_id}")),
        Provider::MusicBrainz => Some(format!("https://musicbrainz.org/artist/{external_id}")),
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

#[expect(clippy::too_many_arguments)]
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
    let external_url = default_provider_artist_url(provider, artist_external_id);
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
    use std::{collections::HashMap, sync::Arc};

    use async_trait::async_trait;
    use chrono::NaiveDate;
    use sea_orm::{ActiveModelBehavior, ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};
    use serde_json::Value;

    use crate::{
        db::{
            album_artist, album_provider_link, artist_provider_link, provider::Provider,
            wanted_status::WantedStatus,
        },
        providers::{
            MetadataProvider, ProviderAlbum, ProviderArtist, ProviderError, ProviderTrack,
            registry::ProviderRegistry,
        },
        test_support,
    };

    struct TestAlbumProvider {
        albums_by_artist: HashMap<String, Vec<ProviderAlbum>>,
    }

    #[async_trait]
    impl MetadataProvider for TestAlbumProvider {
        fn id(&self) -> Provider {
            Provider::Tidal
        }

        fn display_name(&self) -> &str {
            "Test Album Provider"
        }

        async fn search_artists(&self, _query: &str) -> Result<Vec<ProviderArtist>, ProviderError> {
            Ok(Vec::new())
        }

        async fn fetch_albums(
            &self,
            external_artist_id: &str,
        ) -> Result<Vec<ProviderAlbum>, ProviderError> {
            Ok(self
                .albums_by_artist
                .get(external_artist_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn fetch_tracks(
            &self,
            _external_album_id: &str,
        ) -> Result<(Vec<ProviderTrack>, HashMap<String, Value>), ProviderError> {
            Ok((Vec::new(), HashMap::new()))
        }

        async fn fetch_track_info_extra(
            &self,
            _external_track_id: &str,
        ) -> Option<HashMap<String, Value>> {
            None
        }

        fn image_url(&self, image_ref: &str, size: u16) -> String {
            format!("https://example.test/{image_ref}/{size}")
        }

        async fn fetch_cover_art_bytes(&self, _image_ref: &str) -> Option<Vec<u8>> {
            None
        }
    }

    #[test]
    fn default_provider_artist_url_known_providers() {
        assert_eq!(
            super::default_provider_artist_url(Provider::Tidal, "123"),
            Some("https://tidal.com/browse/artist/123".to_string())
        );
        assert_eq!(
            super::default_provider_artist_url(Provider::Deezer, "456"),
            Some("https://www.deezer.com/artist/456".to_string())
        );
        assert_eq!(
            super::default_provider_artist_url(Provider::MusicBrainz, "abc"),
            Some("https://musicbrainz.org/artist/abc".to_string())
        );
        assert!(super::default_provider_artist_url(Provider::None, "x").is_none());
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
    async fn upsert_artist_provider_link_inserts_new_link() {
        let state = test_support::test_state().await;
        let artist = test_support::seed_artist(&state, "Artist", true).await;

        super::upsert_artist_provider_link(
            &state,
            artist.id,
            Provider::Tidal,
            "artist-123",
            Some("https://tidal.com/browse/artist/artist-123".to_string()),
            Some("Artist".to_string()),
        )
        .await
        .expect("insert link");

        let link = artist_provider_link::Entity::find_by_artist_provider_external(
            artist.id,
            Provider::Tidal,
            "artist-123",
        )
        .one(&state.db)
        .await
        .expect("load provider link")
        .expect("link exists");

        assert_eq!(link.external_name.as_deref(), Some("Artist"));
        assert_eq!(
            link.external_url.as_deref(),
            Some("https://tidal.com/browse/artist/artist-123")
        );
    }

    #[tokio::test]
    async fn upsert_artist_provider_link_updates_existing_link() {
        let state = test_support::test_state().await;
        let artist = test_support::seed_artist(&state, "Artist", true).await;

        super::upsert_artist_provider_link(
            &state,
            artist.id,
            Provider::Tidal,
            "artist-123",
            Some("https://old.example.test".to_string()),
            Some("Old Name".to_string()),
        )
        .await
        .expect("insert link");

        super::upsert_artist_provider_link(
            &state,
            artist.id,
            Provider::Tidal,
            "artist-123",
            Some("https://new.example.test".to_string()),
            Some("New Name".to_string()),
        )
        .await
        .expect("update link");

        let links = artist_provider_link::Entity::find()
            .filter(artist_provider_link::Column::ArtistId.eq(artist.id))
            .all(&state.db)
            .await
            .expect("load provider links");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].external_name.as_deref(), Some("New Name"));
        assert_eq!(
            links[0].external_url.as_deref(),
            Some("https://new.example.test")
        );
    }

    #[tokio::test]
    async fn find_or_create_lightweight_artist_persists_artist_and_link() {
        let state = test_support::test_state().await;

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

    #[tokio::test]
    async fn find_or_create_artist_with_provider_link_uses_explicit_link_metadata() {
        let state = test_support::test_state().await;

        let artist_id = super::find_or_create_artist_with_provider_link(
            &state,
            Provider::Deezer,
            "artist-456",
            "Artist",
            Some("https://images.example.test/artist.jpg".to_string()),
            true,
            Some("https://deezer.example.test/artist-456".to_string()),
            Some("Display Artist".to_string()),
        )
        .await
        .expect("create artist");

        let artist = crate::db::artist::Entity::find_by_id(artist_id)
            .one(&state.db)
            .await
            .expect("load artist")
            .expect("artist exists");
        let link = artist_provider_link::Entity::find_by_artist_provider_external(
            artist_id,
            Provider::Deezer,
            "artist-456",
        )
        .one(&state.db)
        .await
        .expect("load provider link")
        .expect("link exists");

        assert!(artist.monitored);
        assert_eq!(
            artist.image_url.as_deref(),
            Some("https://images.example.test/artist.jpg")
        );
        assert_eq!(
            link.external_url.as_deref(),
            Some("https://deezer.example.test/artist-456")
        );
        assert_eq!(link.external_name.as_deref(), Some("Display Artist"));
    }

    #[tokio::test]
    async fn ensure_local_album_reuses_existing_provider_link() {
        let state = test_support::test_state().await;
        let artist_id = super::find_or_create_lightweight_artist(
            &state,
            Provider::Tidal,
            "artist-123",
            "Artist",
        )
        .await
        .expect("create lightweight artist");
        let album = test_support::seed_album(&state, "Existing Album", WantedStatus::Wanted).await;
        test_support::link_album_artist(&state, album.id, artist_id, 0).await;
        album_provider_link::ActiveModel {
            album_id: sea_orm::ActiveValue::Set(album.id),
            provider: sea_orm::ActiveValue::Set(Provider::Tidal),
            provider_album_id: sea_orm::ActiveValue::Set("album-123".to_string()),
            external_url: sea_orm::ActiveValue::Set(None),
            external_name: sea_orm::ActiveValue::Set(Some("Existing Album".to_string())),
            ..album_provider_link::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album provider link");

        let album_id = super::ensure_local_album(
            &state,
            Provider::Tidal,
            "album-123",
            "artist-123",
            "Artist",
            WantedStatus::Acquired,
        )
        .await
        .expect("ensure local album");

        assert_eq!(album_id, album.id);
        assert_eq!(
            crate::db::album::Entity::find()
                .all(&state.db)
                .await
                .expect("reload albums")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn ensure_local_album_creates_album_from_provider_metadata() {
        let mut registry = ProviderRegistry::new();
        registry.register_metadata(Arc::new(TestAlbumProvider {
            albums_by_artist: HashMap::from([(
                "artist-123".to_string(),
                vec![ProviderAlbum {
                    external_id: "album-123".to_string(),
                    title: "Fetched Album".to_string(),
                    album_type: Some("ep".to_string()),
                    release_date: NaiveDate::from_ymd_opt(2024, 4, 20),
                    cover_ref: Some("cover-ref".to_string()),
                    url: Some("https://tidal.example.test/album-123".to_string()),
                    explicit: true,
                }],
            )]),
        }));
        let state = test_support::test_state_with_registry(registry).await;

        let album_id = super::ensure_local_album(
            &state,
            Provider::Tidal,
            "album-123",
            "artist-123",
            "Artist",
            WantedStatus::Wanted,
        )
        .await
        .expect("ensure local album");

        let album = crate::db::album::Entity::find_by_id(album_id)
            .one(&state.db)
            .await
            .expect("load album")
            .expect("album exists");
        let links = album_provider_link::Entity::find()
            .filter(album_provider_link::Column::AlbumId.eq(album_id))
            .all(&state.db)
            .await
            .expect("load provider links");
        let artists = album_artist::Entity::find()
            .filter(album_artist::Column::AlbumId.eq(album_id))
            .all(&state.db)
            .await
            .expect("load album artists");

        assert_eq!(album.title, "Fetched Album");
        assert_eq!(album.album_type, crate::db::album_type::AlbumType::EP);
        assert_eq!(album.release_date, NaiveDate::from_ymd_opt(2024, 4, 20));
        assert_eq!(
            album.cover_url.as_deref(),
            Some("/api/image/tidal/cover-ref/640")
        );
        assert!(album.explicit);
        assert_eq!(album.wanted_status, WantedStatus::Wanted);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].provider_album_id, "album-123");
        assert_eq!(artists.len(), 1);
    }
}
