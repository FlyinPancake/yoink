//! Tidal music provider implementation.
//!
//! Communicates with the hifi-api proxy layer to search, fetch metadata,
//! resolve playback streams, and download cover art from Tidal.
//! Includes automatic instance discovery and failover across multiple
//! upstream hifi-api hosts.

pub(crate) mod api;
pub(crate) mod instances;
pub(crate) mod manifest;
pub(crate) mod models;

use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::debug;

use crate::{
    db::{provider::Provider, quality::Quality},
    providers::NotSupportedSnafu,
};

use self::{
    api::hifi_get_json,
    instances::InstanceCache,
    manifest::{extract_dash_download_payload, extract_download_payload},
    models::*,
};
use super::{
    DownloadSource, DownloadTrackContext, MetadataProvider, PlaybackInfo, ProviderAlbum,
    ProviderArtist, ProviderError, ProviderSearchAlbum, ProviderSearchTrack, ProviderTrack,
};

// ── TidalProvider ───────────────────────────────────────────────────

/// Tidal metadata and download provider.
///
/// Wraps an HTTP client and an [`InstanceCache`] to communicate with
/// upstream hifi-api instances. Supports an optional manual base URL
/// override; when set it is tried first before discovered instances.
pub(crate) struct TidalProvider {
    /// Shared HTTP client used for all upstream requests.
    pub http: reqwest::Client,
    /// Optional user-configured base URL that takes priority over discovery.
    pub manual_base_url: Option<String>,
    /// Cached list of healthy hifi-api instances, refreshed periodically.
    pub instance_cache: Arc<RwLock<InstanceCache>>,
}

impl TidalProvider {
    /// Create a new Tidal provider with the given HTTP client and optional
    /// manual base URL override for the hifi-api proxy.
    pub fn new(http: reqwest::Client, manual_base_url: Option<String>) -> Self {
        Self {
            http,
            manual_base_url,
            instance_cache: Arc::new(RwLock::new(InstanceCache::new())),
        }
    }

    /// Low-level hifi API call with instance failover (exposed for internal use).
    pub async fn hifi_get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: Vec<(String, String)>,
    ) -> Result<T, ProviderError> {
        hifi_get_json(
            &self.http,
            self.manual_base_url.as_deref(),
            &self.instance_cache,
            path,
            query,
        )
        .await
    }
}

#[async_trait]
impl MetadataProvider for TidalProvider {
    fn id(&self) -> Provider {
        Provider::Tidal
    }

    fn display_name(&self) -> &str {
        "Tidal"
    }

    async fn search_artists(&self, query: &str) -> Result<Vec<ProviderArtist>, ProviderError> {
        let parsed = self
            .hifi_get::<HifiResponse>("/search/", vec![("a".to_string(), query.to_string())])
            .await?;

        let artists = parsed
            .data
            .artists
            .map(|paged| paged.items)
            .or(parsed.data.items)
            .unwrap_or_default();

        Ok(artists
            .into_iter()
            .map(|a| {
                // Extract unique role categories as tags.
                let mut tags: Vec<String> = a
                    .artist_roles
                    .iter()
                    .filter_map(|r| r.category.clone())
                    .collect();
                tags.dedup();
                tags.truncate(5);

                // Use first artistTypes entry as artist_type.
                let artist_type = a.artist_types.first().cloned();

                ProviderArtist {
                    external_id: a.id.to_string(),
                    name: a.name,
                    image_ref: a.picture.or(a.selected_album_cover_fallback),
                    url: a.url,
                    disambiguation: None,
                    artist_type,
                    country: None,
                    tags,
                    popularity: a.popularity,
                }
            })
            .collect())
    }

    async fn fetch_albums(
        &self,
        external_artist_id: &str,
    ) -> Result<Vec<ProviderAlbum>, ProviderError> {
        let response = self
            .hifi_get::<HifiArtistAlbumsResponse>(
                "/artist/",
                vec![
                    ("f".to_string(), external_artist_id.to_string()),
                    ("skip_tracks".to_string(), "true".to_string()),
                ],
            )
            .await?;

        Ok(response
            .albums
            .items
            .into_iter()
            .map(|a| {
                let release_date = a.release_date.and_then(|d| d.parse().ok());

                ProviderAlbum {
                    external_id: a.id.to_string(),
                    title: a.title,
                    album_type: a.album_type,
                    release_date,
                    cover_ref: a.cover,
                    url: a.url,
                    explicit: a.explicit.unwrap_or(false),
                }
            })
            .collect())
    }

    async fn fetch_tracks(
        &self,
        external_album_id: &str,
    ) -> Result<(Vec<ProviderTrack>, HashMap<String, serde_json::Value>), ProviderError> {
        let response = self
            .hifi_get::<HifiAlbumResponse>(
                "/album/",
                vec![("id".to_string(), external_album_id.to_string())],
            )
            .await?;

        let album_extra = response.data.extra;
        let tracks = response
            .data
            .items
            .into_iter()
            .enumerate()
            .map(|(idx, item)| {
                let track = match item {
                    HifiAlbumItem::Item { item } => item,
                    HifiAlbumItem::Track(t) => t,
                };
                let explicit = track
                    .extra
                    .get("explicit")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                ProviderTrack {
                    external_id: track.id.to_string(),
                    title: track.title,
                    version: track.version,
                    track_number: track.track_number.unwrap_or((idx + 1) as i32),
                    disc_number: None, // extracted later from extra
                    duration_secs: track.duration.unwrap_or(0),
                    isrc: None,
                    explicit,
                    extra: track.extra,
                }
            })
            .collect();

        Ok((tracks, album_extra))
    }

    async fn fetch_track_info_extra(
        &self,
        external_track_id: &str,
    ) -> Option<HashMap<String, serde_json::Value>> {
        let response = self
            .hifi_get::<serde_json::Value>(
                "/info/",
                vec![("id".to_string(), external_track_id.to_string())],
            )
            .await
            .ok()?;

        let data = response.get("data")?.as_object()?;
        Some(data.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }

    fn validate_image_id(&self, image_id: &str) -> bool {
        // Tidal image IDs are hex UUIDs with hyphens, max 60 chars
        image_id.len() <= 60 && image_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
    }

    fn image_url(&self, image_ref: &str, size: u16) -> String {
        format!(
            "https://resources.tidal.com/images/{}/{size}x{size}.jpg",
            image_ref.replace('-', "/")
        )
    }

    async fn fetch_cover_art_bytes(&self, image_ref: &str) -> Option<Vec<u8>> {
        let url = format!(
            "https://resources.tidal.com/images/{}/1080x1080.jpg",
            image_ref.replace('-', "/")
        );
        let resp = self
            .http
            .get(url)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        resp.bytes().await.ok().map(|b| b.to_vec())
    }

    async fn fetch_artist_image_ref(
        &self,
        external_artist_id: &str,
        name_hint: Option<&str>,
    ) -> Option<String> {
        // Tidal has no "get artist by ID" endpoint that returns the picture,
        // so we search by name and match on the numeric ID.
        let query = name_hint?;
        let parsed = self
            .hifi_get::<HifiResponse>("/search/", vec![("a".to_string(), query.to_string())])
            .await
            .ok()?;

        let artists = parsed
            .data
            .artists
            .map(|paged| paged.items)
            .or(parsed.data.items)
            .unwrap_or_default();

        artists
            .into_iter()
            .find(|a| a.id.to_string() == external_artist_id)
            .and_then(|a| a.picture.or(a.selected_album_cover_fallback))
    }

    async fn search_albums(&self, query: &str) -> Result<Vec<ProviderSearchAlbum>, ProviderError> {
        // The hifi API /search/ endpoint returns albums when queried.
        // We use the same endpoint but extract the albums section.
        let parsed = self
            .hifi_get::<HifiResponse>("/search/", vec![("a".to_string(), query.to_string())])
            .await?;

        let albums = parsed.data.albums.map(|p| p.items).unwrap_or_default();

        Ok(albums
            .into_iter()
            .map(|a| {
                let (artist_name, artist_external_id) = a
                    .artists
                    .first()
                    .map(|ar| (ar.name.clone(), ar.id.to_string()))
                    .unwrap_or_else(|| (crate::api::UNKNOWN_ARTIST.to_string(), String::new()));

                ProviderSearchAlbum {
                    external_id: a.id.to_string(),
                    title: a.title,
                    album_type: a.album_type,
                    release_date: a.release_date,
                    cover_ref: a.cover,
                    url: a.url,
                    explicit: a.explicit.unwrap_or(false),
                    artist_name,
                    artist_external_id,
                }
            })
            .collect())
    }

    async fn search_tracks(&self, query: &str) -> Result<Vec<ProviderSearchTrack>, ProviderError> {
        let parsed = self
            .hifi_get::<HifiResponse>("/search/", vec![("a".to_string(), query.to_string())])
            .await?;

        let tracks = parsed.data.tracks.map(|p| p.items).unwrap_or_default();

        Ok(tracks
            .into_iter()
            .map(|t| {
                let (artist_name, artist_external_id) = t
                    .artists
                    .first()
                    .map(|ar| (ar.name.clone(), ar.id.to_string()))
                    .unwrap_or_else(|| (crate::api::UNKNOWN_ARTIST.to_string(), String::new()));

                let (album_title, album_external_id, album_cover_ref) = t
                    .album
                    .map(|al| (al.title, al.id.to_string(), al.cover))
                    .unwrap_or_else(|| {
                        (crate::api::UNKNOWN_ALBUM.to_string(), String::new(), None)
                    });

                ProviderSearchTrack {
                    external_id: t.id.to_string(),
                    title: t.title,
                    version: t.version,
                    duration_secs: t.duration.unwrap_or(0),
                    isrc: None,
                    explicit: t.explicit.unwrap_or(false),
                    artist_name,
                    artist_external_id,
                    album_title,
                    album_external_id,
                    album_cover_ref,
                }
            })
            .collect())
    }
}

#[async_trait]
impl DownloadSource for TidalProvider {
    fn id(&self) -> Provider {
        Provider::Tidal
    }

    fn requires_linked_provider(&self) -> bool {
        true
    }

    async fn resolve_by_id(
        &self,
        external_track_ids: &[String],
        quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        if external_track_ids.is_empty() {
            return Err(ProviderError::NotFound {
                provider: Provider::Tidal,
                resource: "track".to_string(),
            });
        }

        // FIXME this will only resolve the first track ID
        let external_track_id = &external_track_ids[0];

        if matches!(quality, Quality::Lossless | Quality::HiRes) {
            return self
                .resolve_lossless_by_track_manifest(external_track_id, quality)
                .await;
        }

        let quality_str = quality.as_str().to_string();
        let playback = self
            .hifi_get::<HifiPlaybackResponse>(
                "/track/",
                vec![
                    ("id".to_string(), external_track_id.to_string()),
                    ("quality".to_string(), quality_str),
                ],
            )
            .await?;

        extract_download_payload(&playback.data)
    }

    async fn resolve_by_metadata(
        &self,
        _metadata: &DownloadTrackContext,
        _quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        NotSupportedSnafu {
            provider: Provider::Tidal,
            operation: "resolve_by_metadata".to_string(),
        }
        .fail()
    }
}

impl TidalProvider {
    async fn resolve_lossless_by_track_manifest(
        &self,
        external_track_id: &str,
        quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        let formats = match quality {
            Quality::HiRes => ["FLAC_HIRES", "FLAC"].as_slice(),
            Quality::Lossless => ["FLAC"].as_slice(),
            Quality::High | Quality::Low => unreachable!("only lossless qualities use manifests"),
        };
        let mut query = vec![
            ("id".to_string(), external_track_id.to_string()),
            ("adaptive".to_string(), "true".to_string()),
            ("manifestType".to_string(), "MPEG_DASH".to_string()),
            ("uriScheme".to_string(), "HTTPS".to_string()),
            ("usage".to_string(), "PLAYBACK".to_string()),
        ];
        query.extend(
            formats
                .iter()
                .map(|format| ("formats".to_string(), (*format).to_string())),
        );

        let response = self
            .hifi_get::<HifiTrackManifestsResponse>("/trackManifests/", query)
            .await?;
        let attributes = response.data.data.attributes;
        debug!(
            track_id = external_track_id,
            requested_quality = %quality,
            formats = ?attributes.formats,
            "Resolved Tidal track manifest"
        );

        if attributes
            .formats
            .iter()
            .all(|format| !format.starts_with("FLAC"))
        {
            return Err(ProviderError::InvalidResponse {
                provider: Provider::Tidal,
                reason: format!(
                    "track {external_track_id} requested {quality}, but trackManifests returned formats {:?}",
                    attributes.formats,
                ),
            });
        }

        let manifest_xml = self
            .http
            .get(&attributes.uri)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|source| ProviderError::Reqwest {
                provider: Provider::Tidal,
                operation: "track manifest request".to_string(),
                source,
            })?
            .error_for_status()
            .map_err(|err| ProviderError::Http {
                provider: Provider::Tidal,
                operation: "track manifest status".to_string(),
                error: err.to_string(),
            })?
            .text()
            .await
            .map_err(|source| ProviderError::Reqwest {
                provider: Provider::Tidal,
                operation: "track manifest body".to_string(),
                source,
            })?;

        extract_dash_download_payload(&manifest_xml)
    }
}
