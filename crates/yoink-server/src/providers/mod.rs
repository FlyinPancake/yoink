pub(crate) mod deezer;
#[cfg(test)]
pub mod mock;
pub(crate) mod musicbrainz;
pub(crate) mod registry;
pub(crate) mod soulseek;
pub(crate) mod tidal;

use std::{collections::HashSet, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use chrono::NaiveDate;
use snafu::prelude::*;

use crate::db::{provider::Provider, quality::Quality};

// ── Provider error ──────────────────────────────────────────────────

#[derive(Debug, Snafu)]
pub(crate) enum ProviderError {
    #[snafu(display("{provider} HTTP error during {operation}: {error}"))]
    Http {
        provider: Provider,
        operation: String,
        error: String,
    },
    #[snafu(display("{provider} Reqwest error during {operation}: {source}"))]
    Reqwest {
        provider: Provider,
        operation: String,
        source: reqwest::Error,
    },
    #[snafu(display("{provider} authentication error: {reason}"))]
    Auth { provider: Provider, reason: String },
    #[snafu(display("{provider} rate limited: {reason}"))]
    RateLimited { provider: Provider, reason: String },
    #[snafu(display("{provider} parse error during {operation}: {reason}"))]
    Parse {
        provider: Provider,
        operation: String,
        reason: String,
    },
    #[snafu(display("{provider} JSON parse error during {operation}: {source}"))]
    JsonParse {
        provider: Provider,
        operation: String,
        source: serde_json::Error,
    },
    #[snafu(display("{provider} not found: {resource}"))]
    NotFound {
        provider: Provider,
        resource: String,
    },
    #[snafu(display("{provider} unavailable: {reason}"))]
    Unavailable { provider: Provider, reason: String },
    #[snafu(display("{provider} invalid response: {reason}"))]
    InvalidResponse { provider: Provider, reason: String },
    #[snafu(display("{provider} invalid manual selection: {reason}"))]
    InvalidSelection { provider: Provider, reason: String },

    #[snafu(display("{provider} operation not supported: {operation}"))]
    NotSupported {
        provider: Provider,
        operation: String,
    },
}

// ── Shared provider types ───────────────────────────────────────────

/// An artist returned by a metadata provider search.
#[derive(Debug, Clone)]
pub(crate) struct ProviderArtist {
    pub external_id: String,
    pub name: String,
    pub image_ref: Option<String>,
    pub url: Option<String>,
    /// Short disambiguation comment (e.g. "British electronic duo").
    pub disambiguation: Option<String>,
    /// Artist type: "Person", "Group", "Orchestra", etc.
    pub artist_type: Option<String>,
    /// Country or area name.
    pub country: Option<String>,
    /// Genre/tag names, most relevant first (top 3–5).
    pub tags: Vec<String>,
    /// Popularity percentage (0–100), if available.
    pub popularity: Option<u8>,
}

/// An album returned by a metadata provider.
#[derive(Debug, Clone)]
pub(crate) struct ProviderAlbum {
    pub external_id: String,
    pub title: String,
    pub album_type: Option<String>,
    pub release_date: Option<NaiveDate>,
    pub cover_ref: Option<String>,
    pub url: Option<String>,
    pub explicit: bool,
}

/// A track returned by a metadata provider.
#[derive(Debug, Clone)]
pub(crate) struct ProviderTrack {
    pub external_id: String,
    pub title: String,
    pub version: Option<String>,
    pub track_number: i32,
    pub disc_number: Option<i32>,
    pub duration_secs: i32,
    pub isrc: Option<String>,
    /// Whether the track is marked explicit.
    pub explicit: bool,
}

/// Supplemental track metadata used when writing audio tags.
#[derive(Debug, Clone, Default)]
pub(crate) struct ProviderTrackMetadata {
    pub artists: Vec<String>,
    pub isrc: Option<String>,
    pub copyright: Option<String>,
    pub version: Option<String>,
    pub initial_key: Option<String>,
    pub bpm: Option<u32>,
    pub track_replay_gain: Option<f64>,
    pub track_peak_amplitude: Option<f64>,
}

impl ProviderTrackMetadata {
    /// Fill missing fields from a lower-priority metadata source.
    pub(crate) fn with_fallback(mut self, fallback: Self) -> Self {
        if self.artists.is_empty() {
            self.artists = fallback.artists;
        }
        self.isrc = self.isrc.or(fallback.isrc);
        self.copyright = self.copyright.or(fallback.copyright);
        self.version = self.version.or(fallback.version);
        self.initial_key = self.initial_key.or(fallback.initial_key);
        self.bpm = self.bpm.or(fallback.bpm);
        self.track_replay_gain = self.track_replay_gain.or(fallback.track_replay_gain);
        self.track_peak_amplitude = self.track_peak_amplitude.or(fallback.track_peak_amplitude);
        self
    }
}

/// An album returned by a provider search (includes artist context).
#[derive(Debug, Clone)]
pub(crate) struct ProviderSearchAlbum {
    pub external_id: String,
    pub title: String,
    pub album_type: Option<String>,
    pub release_date: Option<String>,
    pub cover_ref: Option<String>,
    pub url: Option<String>,
    pub explicit: bool,
    /// Primary artist info for display in search results.
    pub artist_name: String,
    pub artist_external_id: String,
}

/// A track returned by a provider search (includes artist + album context).
#[derive(Debug, Clone)]
pub(crate) struct ProviderSearchTrack {
    pub external_id: String,
    pub title: String,
    pub version: Option<String>,
    pub duration_secs: u32,
    pub isrc: Option<String>,
    pub explicit: bool,
    /// Display-ready track artist string.
    pub artist_name: String,
    pub artist_external_id: String,
    /// Album info for display.
    pub album_title: String,
    pub album_external_id: String,
    pub album_cover_ref: Option<String>,
}

/// Resolved playback info for downloading a track.
#[derive(Debug, Clone)]
pub(crate) enum PlaybackInfo {
    /// A single direct download URL.
    DirectUrl(String),
    /// Multiple segment URLs to concatenate (e.g. DASH).
    SegmentUrls(Vec<String>),
    /// A local file path that has already been downloaded.
    LocalFile(PathBuf),
}

/// Supplemental context for download sources that cannot resolve by track ID alone.
#[derive(Debug, Clone)]
pub(crate) struct DownloadTrackContext {
    pub artist_name: String,
    pub album_title: String,
    pub track_title: String,
    pub track_number: Option<u32>,
    pub album_track_count: Option<usize>,
    pub duration_secs: Option<u32>,
}

// ── Traits ──────────────────────────────────────────────────────────

/// Provides metadata: artist search, album listing, track listing, image URLs.
#[async_trait]
pub(crate) trait MetadataProvider: Send + Sync {
    /// Unique provider identifier (e.g. "tidal", "musicbrainz", "deezer").
    fn id(&self) -> Provider;

    /// Search for artists by name.
    async fn search_artists(&self, query: &str) -> Result<Vec<ProviderArtist>, ProviderError>;

    /// Fetch all albums for an artist.
    async fn fetch_albums(
        &self,
        external_artist_id: &str,
    ) -> Result<Vec<ProviderAlbum>, ProviderError>;

    /// Fetch tracks for an album.
    async fn fetch_tracks(
        &self,
        external_album_id: &str,
    ) -> Result<Vec<ProviderTrack>, ProviderError>;

    /// Fetch supplemental metadata for tagging a single track.
    async fn fetch_track_metadata(
        &self,
        _external_track_id: &str,
    ) -> Result<Option<ProviderTrackMetadata>, ProviderError> {
        Ok(None)
    }

    /// Validate an image ID before proxying. Returns `true` if safe.
    /// Override in provider implementations for provider-specific validation.
    fn validate_image_id(&self, image_id: &str) -> bool {
        let _ = image_id;
        false
    }

    /// Build the upstream image URL for a given image ref and size.
    fn image_url(&self, image_ref: &str, size: u16) -> String;

    /// Fetch the image ref for an artist by their external ID.
    /// `name_hint` can be used by providers that need to search by name to find the artist.
    /// Returns a provider-specific image reference that can be passed to `image_url()`.
    /// Default returns `None`; providers can override.
    async fn fetch_artist_image_ref(
        &self,
        _external_artist_id: &str,
        _name_hint: Option<&str>,
    ) -> Option<String> {
        None
    }

    /// Fetch a biographical summary for an artist (plain text).
    /// Default returns `None`; providers can override to source from Wikipedia etc.
    async fn fetch_artist_bio(&self, _external_artist_id: &str) -> Option<String> {
        None
    }

    /// Search for albums by query string.
    /// Providers without this capability return [`ProviderError::NotSupported`].
    async fn search_albums(&self, _query: &str) -> Result<Vec<ProviderSearchAlbum>, ProviderError> {
        Err(ProviderError::NotSupported {
            provider: self.id(),
            operation: "search_albums".to_string(),
        })
    }

    /// Search for tracks by query string.
    /// Providers without this capability return [`ProviderError::NotSupported`].
    async fn search_tracks(&self, _query: &str) -> Result<Vec<ProviderSearchTrack>, ProviderError> {
        Err(ProviderError::NotSupported {
            provider: self.id(),
            operation: "search_tracks".to_string(),
        })
    }
}

/// Resolves playback using provider-linked external track IDs.
#[async_trait]
pub(crate) trait LinkedTrackResolver: Send + Sync {
    /// Source identifier.
    fn id(&self) -> Provider;

    async fn resolve(
        &self,
        external_track_ids: &[String],
        quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError>;
}

/// A single file offered by a peer, surfaced for manual (interactive) search
/// so the user can override automatic candidate selection.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub(crate) struct ManualSearchCandidate {
    pub username: String,
    pub filename: String,
    pub size: i64,
    pub length_secs: Option<u32>,
    pub bit_rate: Option<u32>,
    pub extension: Option<String>,
    /// The automatic matcher's score for this file.
    pub score: i32,
    /// Whether the automatic matcher would consider this file at all.
    pub plausible: bool,
    pub has_free_upload_slot: bool,
    pub queue_length: u32,
}

/// A user-chosen file to download, bypassing automatic candidate selection.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct ManualDownloadSelection {
    pub username: String,
    pub filename: String,
    pub size: i64,
}

/// One file inside a peer's album folder.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct ManualAlbumFile {
    pub filename: String,
    pub size: i64,
    pub length_secs: Option<u32>,
    pub bit_rate: Option<u32>,
    pub extension: Option<String>,
}

/// A peer's album folder surfaced for manual (interactive) album search.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub(crate) struct ManualAlbumCandidate {
    pub username: String,
    pub folder: String,
    pub files: Vec<ManualAlbumFile>,
    /// How many of the album's tracks strictly title-match a file in this
    /// folder.
    pub matched_tracks: u32,
    /// How many of the album's tracks a manual download would actually fetch
    /// from this folder (strict matches plus track-number fallback).
    pub pairable_tracks: u32,
    pub total_size: i64,
    pub score: i32,
    pub has_free_upload_slot: bool,
    pub queue_length: u32,
}

/// A user-chosen album folder to download, bypassing automatic selection.
/// Carries the file listing the user saw so the job doesn't have to search
/// again.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct ManualAlbumSelection {
    pub username: String,
    pub folder: String,
    pub files: Vec<ManualAlbumFile>,
}

/// Resolves playback by searching with locally stored track metadata.
#[async_trait]
pub(crate) trait SearchTrackResolver: Send + Sync {
    /// Source identifier.
    fn id(&self) -> Provider;

    async fn resolve(
        &self,
        metadata: &DownloadTrackContext,
        quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError>;

    /// List every candidate file the search surfaces, scored but unfiltered,
    /// for the user to choose from manually.
    async fn manual_search(
        &self,
        _metadata: &DownloadTrackContext,
        _quality: &Quality,
    ) -> Result<Vec<ManualSearchCandidate>, ProviderError> {
        Err(ProviderError::NotSupported {
            provider: self.id(),
            operation: "manual_search".to_string(),
        })
    }

    /// Download a specific user-chosen file, bypassing candidate selection.
    async fn fetch_file(
        &self,
        _selection: &ManualDownloadSelection,
    ) -> Result<PlaybackInfo, ProviderError> {
        Err(ProviderError::NotSupported {
            provider: self.id(),
            operation: "fetch_file".to_string(),
        })
    }

    /// List candidate album folders for an album's tracks, best-matched
    /// first, for the user to choose from manually.
    async fn manual_album_search(
        &self,
        _tracks: &[DownloadTrackContext],
        _quality: &Quality,
    ) -> Result<Vec<ManualAlbumCandidate>, ProviderError> {
        Err(ProviderError::NotSupported {
            provider: self.id(),
            operation: "manual_album_search".to_string(),
        })
    }

    /// Download one track's file out of a user-chosen album folder.
    async fn fetch_album_file(
        &self,
        _selection: &ManualAlbumSelection,
        _metadata: &DownloadTrackContext,
        _quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        Err(ProviderError::NotSupported {
            provider: self.id(),
            operation: "fetch_album_file".to_string(),
        })
    }
}

/// An enabled download source and the lookup strategy it supports.
#[derive(Clone)]
pub(crate) enum DownloadSource {
    Linked(Arc<dyn LinkedTrackResolver>),
    Search(Arc<dyn SearchTrackResolver>),
}

impl DownloadSource {
    pub fn id(&self) -> Provider {
        match self {
            Self::Linked(source) => source.id(),
            Self::Search(source) => source.id(),
        }
    }

    /// Whether this source can resolve a track with the available provider links.
    pub fn is_available_for(&self, linked_providers: &HashSet<Provider>) -> bool {
        match self {
            Self::Linked(source) => linked_providers.contains(&source.id()),
            Self::Search(_) => true,
        }
    }
}

/// Build an image proxy URL for a given provider and image reference.
pub fn provider_image_url(provider: Provider, image_ref: &str, size: u16) -> String {
    format!("/api/image/{provider}/{image_ref}/{size}")
}

#[cfg(test)]
mod tests {
    use super::ProviderTrackMetadata;

    #[test]
    fn provider_track_metadata_fills_only_missing_fields() {
        let preferred = ProviderTrackMetadata {
            artists: vec!["Provider Artist".to_string()],
            copyright: Some("Provider Copyright".to_string()),
            ..Default::default()
        };
        let fallback = ProviderTrackMetadata {
            artists: vec!["Local Artist".to_string()],
            isrc: Some("LOCALISRC".to_string()),
            copyright: Some("Local Copyright".to_string()),
            version: Some("Local Version".to_string()),
            ..Default::default()
        };

        let merged = preferred.with_fallback(fallback);

        assert_eq!(merged.artists, ["Provider Artist"]);
        assert_eq!(merged.isrc.as_deref(), Some("LOCALISRC"));
        assert_eq!(merged.copyright.as_deref(), Some("Provider Copyright"));
        assert_eq!(merged.version.as_deref(), Some("Local Version"));
    }
}
