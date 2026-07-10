//! HTTP client helpers for talking to hifi-api instances.
//!
//! [`HifiApi`] exposes typed methods for the hifi-api endpoints Yoink uses,
//! while preserving automatic failover across discovered instances.

use std::{sync::Arc, time::Duration};

use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use super::{
    hifi_types::{
        GetAlbumAlbumGetRequestQuery, GetArtistArtistGetRequestQuery, GetInfoInfoGetRequestQuery,
        GetTrackTrackGetRequestQuery, HTTPValidationError, SearchSearchGetRequestQuery,
    },
    instances::{self, InstanceCache},
    models::{
        HifiAlbumResponse, HifiArtistAlbumsResponse, HifiArtistResponse, HifiPlaybackResponse,
        HifiResponse, HifiTrackInfoResponse, HifiTrackManifestsResponse, HifiTrackSearchResponse,
    },
};
use crate::{db::quality::Quality, providers::ProviderError};

#[derive(Clone, Copy)]
pub(crate) enum HifiAudioFormat {
    Flac,
    FlacHires,
}

impl HifiAudioFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Flac => "FLAC",
            Self::FlacHires => "FLAC_HIRES",
        }
    }
}

pub(crate) struct HifiApi<'a> {
    http: &'a reqwest::Client,
    manual_base_url: Option<&'a str>,
    cache: &'a Arc<RwLock<InstanceCache>>,
}

impl<'a> HifiApi<'a> {
    pub(crate) fn new(
        http: &'a reqwest::Client,
        manual_base_url: Option<&'a str>,
        cache: &'a Arc<RwLock<InstanceCache>>,
    ) -> Self {
        Self {
            http,
            manual_base_url,
            cache,
        }
    }

    pub(crate) async fn search_artists(&self, query: &str) -> Result<HifiResponse, ProviderError> {
        self.get("/search/", &search_request(query, SearchKind::Artist))
            .await
    }

    pub(crate) async fn search_albums(&self, query: &str) -> Result<HifiResponse, ProviderError> {
        self.get("/search/", &search_request(query, SearchKind::Album))
            .await
    }

    pub(crate) async fn search_tracks(
        &self,
        query: &str,
    ) -> Result<HifiTrackSearchResponse, ProviderError> {
        self.get("/search/", &search_request(query, SearchKind::Track))
            .await
    }

    pub(crate) async fn artist_albums(
        &self,
        artist_id: &str,
    ) -> Result<HifiArtistAlbumsResponse, ProviderError> {
        let artist_id = parse_tidal_id(artist_id, "artist")?;
        self.get(
            "/artist/",
            &GetArtistArtistGetRequestQuery {
                id: None,
                f: Some(artist_id),
                skip_tracks: Some(true),
            },
        )
        .await
    }

    pub(crate) async fn artist(
        &self,
        artist_id: &str,
    ) -> Result<HifiArtistResponse, ProviderError> {
        let artist_id = parse_tidal_id(artist_id, "artist")?;
        self.get(
            "/artist/",
            &GetArtistArtistGetRequestQuery {
                id: Some(artist_id),
                f: None,
                skip_tracks: None,
            },
        )
        .await
    }

    pub(crate) async fn album(&self, album_id: &str) -> Result<HifiAlbumResponse, ProviderError> {
        let album_id = parse_tidal_id(album_id, "album")?;
        self.get(
            "/album/",
            &GetAlbumAlbumGetRequestQuery {
                id: album_id,
                limit: None,
                offset: None,
            },
        )
        .await
    }

    pub(crate) async fn track_info(
        &self,
        track_id: &str,
    ) -> Result<HifiTrackInfoResponse, ProviderError> {
        let track_id = parse_tidal_id(track_id, "track")?;
        self.get("/info/", &GetInfoInfoGetRequestQuery { id: track_id })
            .await
    }

    pub(crate) async fn track_playback(
        &self,
        track_id: &str,
        quality: &Quality,
    ) -> Result<HifiPlaybackResponse, ProviderError> {
        let track_id = parse_tidal_id(track_id, "track")?;
        self.get(
            "/track/",
            &GetTrackTrackGetRequestQuery {
                id: track_id,
                quality: Some(quality.as_str().to_string()),
                immersiveaudio: None,
            },
        )
        .await
    }

    pub(crate) async fn track_manifests(
        &self,
        track_id: &str,
        formats: &[HifiAudioFormat],
    ) -> Result<HifiTrackManifestsResponse, ProviderError> {
        let query = track_manifests_request(track_id, formats)?;
        self.get("/trackManifests/", &query).await
    }

    async fn get<T: DeserializeOwned, Q: Serialize + ?Sized>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T, ProviderError> {
        hifi_get_json(self.http, self.manual_base_url, self.cache, path, query).await
    }
}

enum SearchKind {
    Artist,
    Album,
    Track,
}

fn search_request(query: &str, kind: SearchKind) -> SearchSearchGetRequestQuery {
    let mut request = SearchSearchGetRequestQuery {
        s: None,
        a: None,
        al: None,
        v: None,
        p: None,
        i: None,
        offset: None,
        limit: None,
    };
    match kind {
        SearchKind::Artist => request.a = Some(query.to_string()),
        SearchKind::Album => request.al = Some(query.to_string()),
        SearchKind::Track => request.s = Some(query.to_string()),
    }
    request
}

fn parse_tidal_id(value: &str, resource: &str) -> Result<i64, ProviderError> {
    let id = value
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| ProviderError::InvalidResponse {
            provider: crate::db::provider::Provider::Tidal,
            reason: format!("invalid Tidal {resource} ID '{value}': expected a positive integer"),
        })?;
    Ok(id)
}

fn track_manifests_request(
    track_id: &str,
    formats: &[HifiAudioFormat],
) -> Result<Vec<(&'static str, String)>, ProviderError> {
    let track_id = parse_tidal_id(track_id, "track")?.to_string();
    let mut query = vec![
        ("id", track_id),
        ("adaptive", "true".to_string()),
        ("manifestType", "MPEG_DASH".to_string()),
        ("uriScheme", "HTTPS".to_string()),
        ("usage", "PLAYBACK".to_string()),
    ];
    query.extend(
        formats
            .iter()
            .map(|format| ("formats", format.as_str().to_string())),
    );
    Ok(query)
}

/// Perform a JSON `GET` against the hifi-api, with automatic instance failover.
///
/// Candidate URLs are resolved via [`instances::candidate_base_urls`] and tried
/// in order. The first instance that returns a valid, deserializable response is
/// promoted to the active instance in the cache.
///
/// Returns a provider error when all candidates fail.
pub(crate) async fn hifi_get_json<T: DeserializeOwned, Q: Serialize + ?Sized>(
    http: &reqwest::Client,
    manual_base_url: Option<&str>,
    cache: &Arc<RwLock<InstanceCache>>,
    path: &str,
    query: &Q,
) -> Result<T, ProviderError> {
    let candidates = instances::candidate_base_urls(manual_base_url, cache, http).await;
    let mut last_error = None;

    for base_url in candidates {
        let response = http
            .get(format!("{base_url}{path}"))
            .query(&query)
            .timeout(Duration::from_secs(8))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status() == reqwest::StatusCode::UNPROCESSABLE_ENTITY => {
                let reason = resp
                    .json::<HTTPValidationError>()
                    .await
                    .ok()
                    .and_then(|error| error.detail)
                    .map(|details| {
                        details
                            .into_iter()
                            .map(|detail| detail.msg)
                            .collect::<Vec<_>>()
                            .join("; ")
                    })
                    .filter(|reason| !reason.is_empty())
                    .unwrap_or_else(|| "hifi-api rejected the request parameters".to_string());
                return Err(ProviderError::InvalidResponse {
                    provider: crate::db::provider::Provider::Tidal,
                    reason,
                });
            }
            Ok(resp) => match resp.error_for_status() {
                Ok(ok) => match ok.json::<T>().await {
                    Ok(parsed) => {
                        instances::set_active_instance(cache, &base_url).await;
                        return Ok(parsed);
                    }
                    Err(err) => {
                        debug!(base_url, error = %err, "Upstream JSON parse failed");
                        last_error = Some(format!("{base_url}: invalid JSON ({err})"));
                    }
                },
                Err(err) => {
                    debug!(base_url, error = %err, "Upstream HTTP status failed");
                    last_error = Some(format!("{base_url}: upstream status error ({err})"));
                }
            },
            Err(err) => {
                debug!(base_url, error = %err, "Upstream request failed");
                last_error = Some(format!("{base_url}: request failed ({err})"));
            }
        }
    }

    let error_msg =
        last_error.unwrap_or_else(|| "No healthy hifi-api instances available".to_string());
    warn!(error = %error_msg, "All hifi-api candidates failed");
    Err(ProviderError::Unavailable {
        provider: crate::db::provider::Provider::Tidal,
        reason: error_msg,
    })
}

#[cfg(test)]
mod tests {
    use axum::{Json, Router, http::StatusCode, routing::get};

    use super::*;
    use crate::providers::tidal::models::HifiAlbumItem;

    #[test]
    fn parse_tidal_id_accepts_positive_numeric_ids() {
        assert_eq!(parse_tidal_id("123456789", "track").unwrap(), 123_456_789);
    }

    #[test]
    fn parse_tidal_id_rejects_invalid_stored_values() {
        for value in ["", "0", "-1", " 123", "123 ", "12a"] {
            let error = parse_tidal_id(value, "track").unwrap_err();
            assert!(
                matches!(error, ProviderError::InvalidResponse { .. }),
                "unexpected error for {value:?}: {error}",
            );
        }
    }

    #[test]
    fn search_requests_use_the_resource_specific_parameter() {
        let artists = search_request("query", SearchKind::Artist);
        assert_eq!(artists.a.as_deref(), Some("query"));
        assert!(artists.al.is_none() && artists.s.is_none());

        let albums = search_request("query", SearchKind::Album);
        assert_eq!(albums.al.as_deref(), Some("query"));
        assert!(albums.a.is_none() && albums.s.is_none());

        let tracks = search_request("query", SearchKind::Track);
        assert_eq!(tracks.s.as_deref(), Some("query"));
        assert!(tracks.a.is_none() && tracks.al.is_none());
    }

    #[test]
    fn track_manifest_formats_are_encoded_as_repeated_query_keys() {
        let query =
            track_manifests_request("123", &[HifiAudioFormat::FlacHires, HifiAudioFormat::Flac])
                .unwrap();
        let request = reqwest::Client::new()
            .get("https://example.com/trackManifests/")
            .query(&query)
            .build()
            .unwrap();
        let formats = request
            .url()
            .query_pairs()
            .filter(|(key, _)| key == "formats")
            .map(|(_, value)| value.into_owned())
            .collect::<Vec<_>>();

        assert_eq!(formats, ["FLAC_HIRES", "FLAC"]);
    }

    #[test]
    fn track_search_response_accepts_the_live_direct_page_shape() {
        let response = serde_json::from_value::<HifiTrackSearchResponse>(serde_json::json!({
            "data": {
                "limit": 1,
                "offset": 0,
                "totalNumberOfItems": 1,
                "items": [{
                    "id": 1550546,
                    "title": "One More Time",
                    "isrc": "GBDUW0000053"
                }]
            }
        }))
        .unwrap();

        assert_eq!(response.data.items[0].id, 1_550_546);
        assert_eq!(response.data.items[0].isrc.as_deref(), Some("GBDUW0000053"));
    }

    #[test]
    fn artist_response_accepts_the_documented_id_shape() {
        let response = serde_json::from_value::<HifiArtistResponse>(serde_json::json!({
            "version": "2.10",
            "artist": {
                "id": 8847,
                "name": "Daft Punk",
                "picture": "c92cf3f5-066f-4f0a-87d0-c2bebff46d36"
            },
            "cover": {
                "750": "https://resources.tidal.com/example/750x750.jpg"
            }
        }))
        .unwrap();

        assert_eq!(response.artist.id, 8847);
        assert_eq!(
            response.artist.picture.as_deref(),
            Some("c92cf3f5-066f-4f0a-87d0-c2bebff46d36"),
        );
    }

    #[test]
    fn album_response_preserves_documented_track_identity_fields() {
        let response = serde_json::from_value::<HifiAlbumResponse>(serde_json::json!({
            "version": "2.10",
            "data": {
                "id": 1550545,
                "title": "Discovery",
                "items": [{
                    "item": {
                        "id": 1550546,
                        "title": "One More Time",
                        "trackNumber": 1,
                        "volumeNumber": 1,
                        "duration": 320,
                        "isrc": "GBDUW0000053",
                        "explicit": false
                    },
                    "type": "track"
                }]
            }
        }))
        .unwrap();
        let HifiAlbumItem::Item { item } = &response.data.items[0] else {
            panic!("expected wrapped album track");
        };

        assert_eq!(item.track_number, Some(1));
        assert_eq!(item.volume_number, Some(1));
        assert_eq!(item.isrc.as_deref(), Some("GBDUW0000053"));
        assert_eq!(item.explicit, Some(false));
    }

    #[test]
    fn track_info_response_maps_taggable_fields() {
        let response = serde_json::from_value::<HifiTrackInfoResponse>(serde_json::json!({
            "version": "2.10",
            "data": {
                "isrc": "GBDUW0000053",
                "copyright": "Copyright",
                "version": "Live",
                "key": "G",
                "bpm": 123,
                "replayGain": -6.83,
                "peak": 0.979767,
                "artists": [{"id": 8847, "name": "Daft Punk"}]
            }
        }))
        .unwrap();

        assert_eq!(response.data.isrc.as_deref(), Some("GBDUW0000053"));
        assert_eq!(response.data.replay_gain, Some(-6.83));
        assert_eq!(response.data.peak, Some(0.979767));
        assert_eq!(response.data.artists[0].name, "Daft Punk");
    }

    #[tokio::test]
    async fn validation_response_is_not_misreported_as_provider_unavailable() {
        let app = Router::new().route(
            "/info/",
            get(|| async {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({
                        "detail": [{
                            "loc": ["query", "id"],
                            "msg": "Input should be a valid integer",
                            "type": "int_parsing"
                        }]
                    })),
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let mut instance_cache = InstanceCache::new();
        instance_cache.last_refresh_instant = Some(std::time::Instant::now());
        let cache = Arc::new(RwLock::new(instance_cache));
        let error = hifi_get_json::<serde_json::Value, _>(
            &reqwest::Client::new(),
            Some(&base_url),
            &cache,
            "/info/",
            &[("id", "invalid")],
        )
        .await
        .unwrap_err();
        server.abort();

        let ProviderError::InvalidResponse { reason, .. } = error else {
            panic!("expected invalid-response error, got {error}");
        };
        assert_eq!(reason, "Input should be a valid integer");
    }
}
