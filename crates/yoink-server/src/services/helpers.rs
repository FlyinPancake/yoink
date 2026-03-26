use sea_orm::{ActiveModelBehavior, ActiveModelTrait};
use tracing::warn;
use uuid::Uuid;

use crate::{
    db::{artist, artist_provider_link, provider::Provider},
    error::AppResult,
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

/// Fetch artist bio from linked metadata providers in background.
pub(crate) fn spawn_fetch_artist_bio(state: &AppState, artist_id: Uuid) {
    // TODO: re-enable this once i have time

    // let s = state.clone();
    // info!(%artist_id, "Spawning background bio fetch");
    // tokio::spawn(async move {
    //     let links = match db::load_artist_provider_links(&s.sqlite, artist_id).await {
    //         Ok(l) => l,
    //         Err(e) => {
    //             warn!(%artist_id, error = %e, "Failed to load provider links for bio fetch");
    //             return;
    //         }
    //     };

    //     if links.is_empty() {
    //         info!(%artist_id, "No provider links found, skipping bio fetch");
    //         return;
    //     }

    //     info!(%artist_id, link_count = links.len(), "Attempting bio fetch from linked providers");

    //     for link in &links {
    //         let provider_name = &link.provider;
    //         let external_id = &link.external_id;

    //         let Some(provider) = s.registry.metadata_provider(provider_name) else {
    //             info!(
    //                 %artist_id,
    //                 provider = %provider_name,
    //                 "Provider not available as metadata source, skipping"
    //             );
    //             continue;
    //         };

    //         info!(
    //             %artist_id,
    //             provider = %provider_name,
    //             %external_id,
    //             "Fetching bio from provider"
    //         );

    //         match provider.fetch_artist_bio(external_id).await {
    //             Some(bio) => {
    //                 let bio_len = bio.len();
    //                 if let Err(e) = db::update_artist_bio(&s.sqlite, artist_id, Some(&bio)).await {
    //                     warn!(
    //                         %artist_id,
    //                         provider = %provider_name,
    //                         error = %e,
    //                         "Failed to persist fetched bio to database"
    //                     );
    //                     return;
    //                 }
    //                 {
    //                     let mut artists = s.monitored_artists.write().await;
    //                     if let Some(a) = artists.iter_mut().find(|a| a.id == artist_id) {
    //                         a.bio = Some(bio);
    //                     }
    //                 }
    //                 info!(
    //                     %artist_id,
    //                     provider = %provider_name,
    //                     bio_len,
    //                     "Successfully fetched and saved artist bio"
    //                 );
    //                 s.notify_sse();
    //                 return;
    //             }
    //             None => {
    //                 info!(
    //                     %artist_id,
    //                     provider = %provider_name,
    //                     %external_id,
    //                     "Provider returned no bio"
    //                 );
    //             }
    //         }
    //     }

    //     info!(
    //         %artist_id,
    //         "No provider returned a bio for this artist"
    //     );
    // });
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

/// Find an existing artist by provider link, or create a new lightweight
/// (unmonitored) artist with a single provider link.
pub(crate) async fn find_or_create_lightweight_artist(
    state: &AppState,
    provider: Provider,
    artist_external_id: &str,
    artist_name: &str,
) -> AppResult<Uuid> {
    if let Some(existing) =
        artist_provider_link::Entity::find_by_provider_external(provider, artist_external_id)
            .one(&state.db)
            .await?
    {
        return Ok(existing.artist_id);
    }

    let external_url = default_provider_artist_url(&provider.to_string(), artist_external_id);

    let artist = artist::ActiveModel {
        name: sea_orm::ActiveValue::Set(artist_name.to_string()),
        image_url: sea_orm::ActiveValue::Set(None),
        bio: sea_orm::ActiveValue::Set(None),
        monitored: sea_orm::ActiveValue::Set(false),
        ..artist::ActiveModel::new()
    }
    .insert(&state.db)
    .await?;

    let link = artist_provider_link::ActiveModel {
        artist_id: sea_orm::ActiveValue::Set(artist.id),
        provider: sea_orm::ActiveValue::Set(provider),
        external_id: sea_orm::ActiveValue::Set(artist_external_id.to_string()),
        external_url: sea_orm::ActiveValue::Set(external_url),
        external_name: sea_orm::ActiveValue::Set(Some(artist_name.to_string())),
        ..artist_provider_link::ActiveModel::new()
    };
    link.insert(&state.db).await?;

    Ok(artist.id)
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
