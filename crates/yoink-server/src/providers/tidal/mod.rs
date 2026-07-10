//! Tidal music provider implementation.
//!
//! Communicates with the hifi-api proxy layer to search, fetch metadata,
//! resolve playback streams, and download cover art from Tidal.
//! Includes automatic instance discovery and failover across multiple
//! upstream hifi-api hosts.

pub(crate) mod api;
#[allow(clippy::needless_return)]
mod hifi_types;
pub(crate) mod instances;
pub(crate) mod manifest;
pub(crate) mod models;

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::db::{provider::Provider, quality::Quality};

use self::{
    api::{HifiApi, HifiAudioFormat},
    instances::InstanceCache,
    manifest::{extract_dash_download_payload, extract_download_payload},
    models::*,
};
use super::{
    LinkedTrackResolver, MetadataProvider, PlaybackInfo, ProviderAlbum, ProviderArtist,
    ProviderError, ProviderSearchAlbum, ProviderSearchTrack, ProviderTrack, ProviderTrackMetadata,
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

    fn hifi(&self) -> HifiApi<'_> {
        HifiApi::new(
            &self.http,
            self.manual_base_url.as_deref(),
            &self.instance_cache,
        )
    }
}

#[async_trait]
impl MetadataProvider for TidalProvider {
    fn id(&self) -> Provider {
        Provider::Tidal
    }

    async fn search_artists(&self, query: &str) -> Result<Vec<ProviderArtist>, ProviderError> {
        let parsed = self.hifi().search_artists(query).await?;

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
        let response = self.hifi().artist_albums(external_artist_id).await?;

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
    ) -> Result<Vec<ProviderTrack>, ProviderError> {
        let response = self.hifi().album(external_album_id).await?;

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
                ProviderTrack {
                    external_id: track.id.to_string(),
                    title: track.title,
                    version: track.version,
                    track_number: track.track_number.unwrap_or((idx + 1) as i32),
                    disc_number: track.volume_number,
                    duration_secs: track.duration.unwrap_or(0),
                    isrc: track.isrc.filter(|isrc| !isrc.is_empty()),
                    explicit: track.explicit.unwrap_or(false),
                }
            })
            .collect();

        Ok(tracks)
    }

    async fn fetch_track_metadata(
        &self,
        external_track_id: &str,
    ) -> Result<Option<ProviderTrackMetadata>, ProviderError> {
        let response = self.hifi().track_info(external_track_id).await?;
        let data = response.data;
        Ok(Some(ProviderTrackMetadata {
            artists: data.artists.into_iter().map(|artist| artist.name).collect(),
            isrc: data.isrc.filter(|isrc| !isrc.is_empty()),
            copyright: data.copyright,
            version: data.version.filter(|version| !version.is_empty()),
            initial_key: data.key,
            bpm: data.bpm,
            track_replay_gain: data.replay_gain,
            track_peak_amplitude: data.peak,
        }))
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

    async fn fetch_artist_image_ref(
        &self,
        external_artist_id: &str,
        _name_hint: Option<&str>,
    ) -> Option<String> {
        let response = self.hifi().artist(external_artist_id).await.ok()?;
        response
            .artist
            .picture
            .or(response.artist.selected_album_cover_fallback)
    }

    async fn search_albums(&self, query: &str) -> Result<Vec<ProviderSearchAlbum>, ProviderError> {
        // The hifi API /search/ endpoint returns albums when queried.
        // We use the same endpoint but extract the albums section.
        let parsed = self.hifi().search_albums(query).await?;

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
        let parsed = self.hifi().search_tracks(query).await?;

        let tracks = parsed.data.items;

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
                    isrc: t.isrc.filter(|isrc| !isrc.is_empty()),
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
impl LinkedTrackResolver for TidalProvider {
    fn id(&self) -> Provider {
        Provider::Tidal
    }

    async fn resolve(
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

        let mut failures = Vec::new();
        for external_track_id in external_track_ids {
            let result = if matches!(quality, Quality::Lossless | Quality::HiRes) {
                self.resolve_lossless_by_track_manifest(external_track_id, quality)
                    .await
            } else {
                self.resolve_track_playback(external_track_id, quality)
                    .await
            };

            match result {
                Ok(playback) => return Ok(playback),
                Err(err) => {
                    warn!(
                        track_id = external_track_id,
                        requested_quality = %quality,
                        error = %err,
                        "Tidal track candidate could not be resolved"
                    );
                    failures.push(format!("{external_track_id}: {err}"));
                }
            }
        }

        Err(ProviderError::InvalidResponse {
            provider: Provider::Tidal,
            reason: format!(
                "none of {} Tidal track candidates could be resolved at {quality}: {}",
                external_track_ids.len(),
                failures.join("; "),
            ),
        })
    }
}

impl TidalProvider {
    async fn resolve_track_playback(
        &self,
        external_track_id: &str,
        quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        let playback = self
            .hifi()
            .track_playback(external_track_id, quality)
            .await?;

        extract_download_payload(&playback.data)
    }

    async fn resolve_lossless_by_track_manifest(
        &self,
        external_track_id: &str,
        quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        let formats = match quality {
            Quality::HiRes => [HifiAudioFormat::FlacHires, HifiAudioFormat::Flac].as_slice(),
            Quality::Lossless => [HifiAudioFormat::Flac].as_slice(),
            Quality::High | Quality::Low => unreachable!("only lossless qualities use manifests"),
        };
        let response = self
            .hifi()
            .track_manifests(external_track_id, formats)
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
