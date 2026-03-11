use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    db,
    error::{AppError, AppResult},
    services,
    state::AppState,
};

/// Generate a default external URL for an artist on a given provider.
pub(super) fn default_provider_artist_url(provider: &str, external_id: &str) -> Option<String> {
    match provider {
        "tidal" => Some(format!("https://tidal.com/browse/artist/{external_id}")),
        "deezer" => Some(format!("https://www.deezer.com/artist/{external_id}")),
        "musicbrainz" => Some(format!("https://musicbrainz.org/artist/{external_id}")),
        _ => None,
    }
}

/// Fetch artist bio from linked metadata providers in background.
pub(super) fn spawn_fetch_artist_bio(state: &AppState, artist_id: Uuid) {
    let s = state.clone();
    info!(%artist_id, "Spawning background bio fetch");
    tokio::spawn(async move {
        let links = match db::load_artist_provider_links(&s.db, artist_id).await {
            Ok(l) => l,
            Err(e) => {
                warn!(%artist_id, error = %e, "Failed to load provider links for bio fetch");
                return;
            }
        };

        if links.is_empty() {
            info!(%artist_id, "No provider links found, skipping bio fetch");
            return;
        }

        info!(%artist_id, link_count = links.len(), "Attempting bio fetch from linked providers");

        for link in &links {
            let provider_name = &link.provider;
            let external_id = &link.external_id;

            let Some(provider) = s.registry.metadata_provider(provider_name) else {
                info!(
                    %artist_id,
                    provider = %provider_name,
                    "Provider not available as metadata source, skipping"
                );
                continue;
            };

            info!(
                %artist_id,
                provider = %provider_name,
                %external_id,
                "Fetching bio from provider"
            );

            match provider.fetch_artist_bio(external_id).await {
                Some(bio) => {
                    let bio_len = bio.len();
                    if let Err(e) = db::update_artist_bio(&s.db, artist_id, Some(&bio)).await {
                        warn!(
                            %artist_id,
                            provider = %provider_name,
                            error = %e,
                            "Failed to persist fetched bio to database"
                        );
                        return;
                    }
                    {
                        let mut artists = s.monitored_artists.write().await;
                        if let Some(a) = artists.iter_mut().find(|a| a.id == artist_id) {
                            a.bio = Some(bio);
                        }
                    }
                    info!(
                        %artist_id,
                        provider = %provider_name,
                        bio_len,
                        "Successfully fetched and saved artist bio"
                    );
                    s.notify_sse();
                    return;
                }
                None => {
                    info!(
                        %artist_id,
                        provider = %provider_name,
                        %external_id,
                        "Provider returned no bio"
                    );
                }
            }
        }

        info!(
            %artist_id,
            "No provider returned a bio for this artist"
        );
    });
}

pub(super) fn spawn_recompute_artist_match_suggestions(state: &AppState, artist_id: Uuid) {
    let s = state.clone();
    tokio::spawn(async move {
        if let Err(err) = services::recompute_artist_match_suggestions(&s, artist_id).await {
            warn!(artist_id = %artist_id, error = %err, "Background match recompute failed");
        }
        s.notify_sse();
    });
}

/// Find an existing artist by provider link, or create a new lightweight
/// (unmonitored) artist with a single provider link.
pub(super) async fn find_or_create_lightweight_artist(
    state: &AppState,
    provider: &str,
    artist_external_id: &str,
    artist_name: &str,
) -> AppResult<Uuid> {
    // Check if artist already exists via provider link.
    if let Ok(Some(id)) =
        db::find_artist_by_provider_link(&state.db, provider, artist_external_id).await
    {
        return Ok(id);
    }

    let new_id = Uuid::now_v7();
    let external_url = default_provider_artist_url(provider, artist_external_id);
    let artist = yoink_shared::MonitoredArtist {
        id: new_id,
        name: artist_name.to_string(),
        image_url: None,
        bio: None,
        monitored: false, // lightweight — no auto-sync
        added_at: Utc::now(),
    };
    db::upsert_artist(&state.db, &artist).await?;
    {
        let mut artists = state.monitored_artists.write().await;
        artists.push(artist);
    }

    let link = db::ArtistProviderLink {
        id: Uuid::now_v7(),
        artist_id: new_id,
        provider: provider.to_string(),
        external_id: artist_external_id.to_string(),
        external_url,
        external_name: Some(artist_name.to_string()),
        image_ref: None,
    };
    db::upsert_artist_provider_link(&state.db, &link).await?;

    Ok(new_id)
}

/// Fetch tracks for an album from a provider and store them in the DB.
/// If `monitor_all` is true, every track is marked as monitored.
pub(super) async fn store_album_tracks(
    state: &AppState,
    provider: &str,
    external_album_id: &str,
    album_id: Uuid,
    monitor_all: bool,
) -> AppResult<()> {
    let prov = state.registry.metadata_provider(provider).ok_or_else(|| {
        AppError::unavailable(
            "metadata provider",
            format!("unknown provider '{provider}'"),
        )
    })?;

    let (tracks, _album_extra) = prov.fetch_tracks(external_album_id).await?;

    for track in tracks {
        let ext_id = track.external_id.clone();

        // Skip if this exact provider+external_id track already exists.
        let existing = db::find_track_by_provider_link(&state.db, provider, &ext_id)
            .await
            .ok()
            .flatten();
        if existing.is_some() {
            continue;
        }

        let track_number = track.track_number.max(1);
        let track_info = track.into_track_info(crate::providers::LocalTrackOverrides {
            track_number: Some(track_number),
            monitored: monitor_all,
            ..Default::default()
        });

        db::upsert_track(&state.db, &track_info, album_id).await?;
        db::upsert_track_provider_link(&state.db, track_info.id, provider, &ext_id).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
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
}
