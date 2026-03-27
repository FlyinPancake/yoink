use std::collections::{HashMap, HashSet};

use sea_orm::{
    ActiveValue::Set, ColumnTrait, EntityLoaderTrait, EntityTrait, IntoActiveModel, QueryFilter,
};
use tracing::{info, warn};
use url::Url;
use uuid::Uuid;
use yoink_shared::ArtistImageOption;

use crate::{
    db::{self, album_artist, artist, artist_provider_link, provider::Provider},
    error::{AppError, AppResult},
    providers::provider_image_url,
    state::AppState,
};

use super::helpers;

const ARTIST_IMAGE_SIZE: u16 = 320;

pub(crate) async fn add_artist(
    state: &AppState,
    name: String,
    provider: Provider,
    external_id: String,
    image_url: Option<String>,
    external_url: Option<Url>,
) -> AppResult<()> {
    let external_url = external_url
        .map(|u| u.to_string())
        .or_else(|| helpers::default_provider_artist_url(provider, &external_id));
    let external_name = name.clone();

    let artist_id = helpers::find_or_create_artist_with_provider_link(
        state,
        provider,
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
    let album_ids: Vec<Uuid> = album_artists.iter().map(|aa| aa.album_id).collect();
    let related_album_artists =
        album_artist::Entity::find_by_album_ids_ordered(album_ids.iter().copied())
            .all(&state.db)
            .await?;
    let mut artist_counts_by_album = HashMap::new();
    for junction in related_album_artists {
        *artist_counts_by_album
            .entry(junction.album_id)
            .or_insert(0usize) += 1;
    }

    let albums_to_delete: Vec<Uuid> = album_artists
        .iter()
        .filter_map(|aa| {
            (artist_counts_by_album
                .get(&aa.album_id)
                .copied()
                .unwrap_or_default()
                <= 1)
                .then_some(aa.album_id)
        })
        .collect();

    let albums_by_id: HashMap<Uuid, db::album::Model> =
        if remove_files && !albums_to_delete.is_empty() {
            db::album::Entity::find()
                .filter(db::album::Column::Id.is_in(albums_to_delete.iter().copied()))
                .all(&state.db)
                .await?
                .into_iter()
                .map(|album| (album.id, album))
                .collect()
        } else {
            HashMap::new()
        };

    for album_id in albums_to_delete {
        if remove_files {
            match albums_by_id.get(&album_id) {
                Some(album) => {
                    super::downloads::remove_downloaded_album_files(state, album).await?;
                }
                None => {
                    warn!(artist_id = %artist_id, album_id = %album_id, "Album disappeared before files could be removed");
                }
            };
        }
        db::album::Entity::delete_by_id(album_id)
            .exec(&state.db)
            .await?;
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
    let mut model = artist::Entity::load()
        .filter_by_id(artist_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("artist", Some(artist_id.to_string())))?
        .into_active_model();

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
    let model = artist::Entity::find_by_id(artist_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("artist", Some(artist_id.to_string())))?
        .into_active_model()
        .into_ex();

    model.set_monitored(monitored).update(&state.db).await?;

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

    let model = artist::Entity::find_by_id(artist_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("artist", Some(artist_id.to_string())))?
        .into_active_model()
        .into_ex();

    model.set_bio(None).update(&state.db).await?;

    state.notify_sse();
    helpers::spawn_fetch_artist_bio(state, artist_id);
    Ok(())
}

pub(crate) async fn get_artist_images(
    state: &AppState,
    artist_id: Uuid,
) -> AppResult<Vec<ArtistImageOption>> {
    let artist = artist::Entity::load()
        .filter_by_id(artist_id)
        .with(artist_provider_link::Entity)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("artist", Some(artist_id.to_string())))?;

    let mut images = Vec::new();
    let mut seen_urls = HashSet::new();

    for link in artist.provider_links.into_iter() {
        let Some(provider) = state.registry.metadata_provider(link.provider) else {
            continue;
        };

        let Some(image_ref) = provider
            .fetch_artist_image_ref(&link.external_id, Some(&artist.name))
            .await
        else {
            continue;
        };

        let image_url = provider_image_url(link.provider, &image_ref, ARTIST_IMAGE_SIZE);
        if seen_urls.insert(image_url.clone()) {
            images.push(ArtistImageOption {
                provider: link.provider.to_string(),
                image_url,
            });
        }
    }

    Ok(images)
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
    provider: Provider,
    external_id: String,
    external_url: Option<String>,
    external_name: Option<String>,
    _image_ref: Option<String>,
) -> AppResult<()> {
    let external_url =
        external_url.or_else(|| helpers::default_provider_artist_url(provider, &external_id));

    helpers::upsert_artist_provider_link(
        state,
        artist_id,
        provider,
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use sea_orm::{ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, EntityTrait};
    use serde_json::Value;
    use tokio::time::sleep;

    use super::*;
    use crate::{
        app_config::AuthConfig,
        db::{
            album, album_artist, album_type::AlbumType, provider::Provider, track,
            wanted_status::WantedStatus,
        },
        providers::{
            MetadataProvider, ProviderAlbum, ProviderArtist, ProviderError, ProviderTrack,
            registry::ProviderRegistry,
        },
    };

    async fn test_state() -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-artist-service-test-{}.db?mode=rwc",
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

    async fn test_state_with_music_root(music_root: PathBuf) -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-artist-remove-test-{}.db?mode=rwc",
            uuid::Uuid::now_v7()
        );

        AppState::new(
            music_root,
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

    struct TestArtistImageProvider;

    #[async_trait]
    impl MetadataProvider for TestArtistImageProvider {
        fn id(&self) -> Provider {
            Provider::Deezer
        }

        fn display_name(&self) -> &str {
            "Test Provider"
        }

        async fn search_artists(&self, _query: &str) -> Result<Vec<ProviderArtist>, ProviderError> {
            Ok(Vec::new())
        }

        async fn fetch_albums(
            &self,
            _external_artist_id: &str,
        ) -> Result<Vec<ProviderAlbum>, ProviderError> {
            Ok(Vec::new())
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

        async fn fetch_artist_image_ref(
            &self,
            external_artist_id: &str,
            _name_hint: Option<&str>,
        ) -> Option<String> {
            Some(format!("artist:{external_artist_id}"))
        }

        async fn fetch_artist_bio(&self, external_artist_id: &str) -> Option<String> {
            Some(format!("Bio for {external_artist_id}"))
        }
    }

    async fn test_state_with_artist_image_provider() -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-artist-image-test-{}.db?mode=rwc",
            uuid::Uuid::now_v7()
        );

        let mut registry = ProviderRegistry::new();
        registry.register_metadata(Arc::new(TestArtistImageProvider));

        AppState::new(
            PathBuf::from("./music"),
            crate::db::quality::Quality::Lossless,
            false,
            1,
            &db_path,
            registry,
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
    async fn get_artist_images_returns_provider_candidates() {
        let state = test_state_with_artist_image_provider().await;

        let artist_id = helpers::find_or_create_lightweight_artist(
            &state,
            Provider::Deezer,
            "123",
            "Test Artist",
        )
        .await
        .expect("create artist");

        let images = get_artist_images(&state, artist_id)
            .await
            .expect("get artist images");

        assert_eq!(images.len(), 1);
        assert_eq!(images[0].provider, "deezer");
        assert_eq!(images[0].image_url, "/api/image/deezer/artist:123/320");
    }

    #[tokio::test]
    async fn get_artist_images_errors_for_missing_artist() {
        let state = test_state().await;

        let err = get_artist_images(&state, Uuid::now_v7())
            .await
            .expect_err("missing artist should error");

        assert!(matches!(err, AppError::NotFound { .. }));
    }

    #[tokio::test]
    async fn fetch_artist_bio_persists_provider_bio() {
        let state = test_state_with_artist_image_provider().await;

        let artist_id = helpers::find_or_create_lightweight_artist(
            &state,
            Provider::Deezer,
            "123",
            "Test Artist",
        )
        .await
        .expect("create artist");

        super::fetch_artist_bio(&state, artist_id)
            .await
            .expect("request manual bio fetch");

        let mut persisted_bio = None;
        for _ in 0..20 {
            persisted_bio = artist::Entity::find_by_id(artist_id)
                .one(&state.db)
                .await
                .expect("reload artist")
                .and_then(|artist| artist.bio);
            if persisted_bio.is_some() {
                break;
            }
            sleep(Duration::from_millis(25)).await;
        }

        assert_eq!(persisted_bio.as_deref(), Some("Bio for 123"));
    }

    #[tokio::test]
    async fn remove_artist_with_remove_files_deletes_managed_album_files() {
        let music_root = tempfile::tempdir().expect("create music root");
        let state = test_state_with_music_root(music_root.path().to_path_buf()).await;

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
            wanted_status: Set(WantedStatus::Acquired),
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

        let relative_path = "Test Artist/Test Album/01 - Track 1.flac".to_string();
        let absolute_path = music_root.path().join(&relative_path);
        std::fs::create_dir_all(absolute_path.parent().expect("parent dir")).expect("create dirs");
        std::fs::write(&absolute_path, b"audio").expect("write track");

        track::ActiveModel {
            title: Set("Track 1".to_string()),
            version: Set(None),
            disc_number: Set(Some(1)),
            track_number: Set(Some(1)),
            duration: Set(Some(180)),
            album_id: Set(album.id),
            explicit: Set(false),
            isrc: Set(None),
            root_folder_id: Set(None),
            status: Set(WantedStatus::Acquired),
            file_path: Set(Some(relative_path)),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert track");

        super::remove_artist(&state, artist.id, true)
            .await
            .expect("remove artist");

        assert!(
            !absolute_path.exists(),
            "expected managed album file to be removed"
        );
        assert!(
            artist::Entity::find_by_id(artist.id)
                .one(&state.db)
                .await
                .expect("reload artist")
                .is_none()
        );
        assert!(
            album::Entity::find_by_id(album.id)
                .one(&state.db)
                .await
                .expect("reload album")
                .is_none()
        );
    }
}
