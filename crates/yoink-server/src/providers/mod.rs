pub(crate) mod deezer;
pub(crate) mod musicbrainz;
pub(crate) mod registry;
pub(crate) mod soulseek;
pub(crate) mod tidal;

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::Value;
use thiserror::Error;

use crate::db::{provider::Provider, quality::Quality};

// ── Provider error ──────────────────────────────────────────────────

#[derive(Debug, Clone, Error)]
pub(crate) enum ProviderError {
    #[error("{provider} HTTP error during {operation}: {reason}")]
    Http {
        provider: String,
        operation: String,
        reason: String,
    },
    #[error("{provider} authentication error: {reason}")]
    Auth { provider: String, reason: String },
    #[error("{provider} rate limited: {reason}")]
    RateLimited { provider: String, reason: String },
    #[error("{provider} parse error during {operation}: {reason}")]
    Parse {
        provider: String,
        operation: String,
        reason: String,
    },
    #[error("{provider} not found: {resource}")]
    NotFound { provider: String, resource: String },
    #[error("{provider} unavailable: {reason}")]
    Unavailable { provider: String, reason: String },
    #[error("{provider} invalid response: {reason}")]
    InvalidResponse { provider: String, reason: String },
}

impl ProviderError {
    pub(crate) fn http(
        provider: impl Into<String>,
        operation: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::Http {
            provider: provider.into(),
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn auth(provider: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Auth {
            provider: provider.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn rate_limited(provider: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::RateLimited {
            provider: provider.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn parse(
        provider: impl Into<String>,
        operation: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::Parse {
            provider: provider.into(),
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn not_found(provider: impl Into<String>, resource: impl Into<String>) -> Self {
        Self::NotFound {
            provider: provider.into(),
            resource: resource.into(),
        }
    }

    pub(crate) fn unavailable(provider: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Unavailable {
            provider: provider.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn invalid_response(provider: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidResponse {
            provider: provider.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn is_rate_limited(&self) -> bool {
        matches!(self, Self::RateLimited { .. })
    }
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

/// A minimal artist reference attached to a provider album.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct ProviderAlbumArtist {
    pub external_id: String,
    pub name: String,
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
    /// Provider-specific extra metadata (for tagging).
    pub extra: HashMap<String, Value>,
}

// TODO: remove impl, and convert the tests

#[cfg(test)]
/// Local-state overrides applied when converting a [`ProviderTrack`] into
/// a [`yoink_shared::TrackInfo`].  Fields default to sensible "fresh track"
/// values so callers only need to set what differs.
pub(crate) struct LocalTrackOverrides {
    pub id: uuid::Uuid,
    pub quality_override: Option<yoink_shared::Quality>,
    pub file_path: Option<String>,
    pub monitored: bool,
    pub acquired: bool,
    /// Override disc number (e.g. from file metadata). `None` uses the
    /// provider value.
    pub disc_number: Option<i32>,
    /// Override track number. `None` uses the provider value.
    pub track_number: Option<i32>,
    /// Override explicit flag. `None` uses the provider value.
    pub explicit: Option<bool>,
}

// TODO: remove impl, and convert the tests

#[cfg(test)]
impl Default for LocalTrackOverrides {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::now_v7(),
            quality_override: None,
            file_path: None,
            monitored: false,
            acquired: false,
            disc_number: None,
            track_number: None,
            explicit: None,
        }
    }
}

// TODO: remove impl, and convert the tests

#[cfg(test)]
impl ProviderTrack {
    /// Convert this provider track into a [`TrackInfo`], applying
    /// local-state overrides for fields that differ by call site.
    pub(crate) fn into_track_info(self, overrides: LocalTrackOverrides) -> yoink_shared::TrackInfo {
        let secs = self.duration_secs;
        yoink_shared::TrackInfo {
            id: overrides.id,
            title: self.title,
            version: self.version,
            disc_number: overrides.disc_number.or(self.disc_number).unwrap_or(1),
            track_number: overrides.track_number.unwrap_or(self.track_number),
            duration_secs: secs,
            isrc: self.isrc,
            explicit: overrides.explicit.unwrap_or(self.explicit),
            quality_override: overrides.quality_override,
            track_artist: None,
            file_path: overrides.file_path,
            monitored: overrides.monitored,
            acquired: overrides.acquired,
        }
    }

    /// Borrowing variant of [`into_track_info`](Self::into_track_info)
    /// for call sites that continue using the `ProviderTrack` afterward.
    pub(crate) fn to_track_info(&self, overrides: LocalTrackOverrides) -> yoink_shared::TrackInfo {
        let secs = self.duration_secs;
        yoink_shared::TrackInfo {
            id: overrides.id,
            title: self.title.clone(),
            version: self.version.clone(),
            disc_number: overrides.disc_number.or(self.disc_number).unwrap_or(1),
            track_number: overrides.track_number.unwrap_or(self.track_number),
            duration_secs: secs,
            isrc: self.isrc.clone(),
            explicit: overrides.explicit.unwrap_or(self.explicit),
            quality_override: overrides.quality_override,
            track_artist: None,
            file_path: overrides.file_path,
            monitored: overrides.monitored,
            acquired: overrides.acquired,
        }
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

    /// Human-readable display name.
    #[allow(dead_code)]
    fn display_name(&self) -> &str;

    /// Search for artists by name.
    async fn search_artists(&self, query: &str) -> Result<Vec<ProviderArtist>, ProviderError>;

    /// Fetch all albums for an artist.
    async fn fetch_albums(
        &self,
        external_artist_id: &str,
    ) -> Result<Vec<ProviderAlbum>, ProviderError>;

    /// Fetch tracks for an album (with extra metadata for tagging).
    async fn fetch_tracks(
        &self,
        external_album_id: &str,
    ) -> Result<(Vec<ProviderTrack>, HashMap<String, Value>), ProviderError>;

    /// Fetch extra metadata for a single track (ISRC, BPM, key, etc.).
    async fn fetch_track_info_extra(
        &self,
        external_track_id: &str,
    ) -> Option<HashMap<String, Value>>;

    /// Validate an image ID before proxying. Returns `true` if safe.
    /// Override in provider implementations for provider-specific validation.
    fn validate_image_id(&self, image_id: &str) -> bool {
        let _ = image_id;
        true
    }

    /// Build the upstream image URL for a given image ref and size.
    fn image_url(&self, image_ref: &str, size: u16) -> String;

    /// Fetch cover art bytes for an image ref (full resolution).
    async fn fetch_cover_art_bytes(&self, image_ref: &str) -> Option<Vec<u8>>;

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
    /// Default returns empty; providers can override.
    async fn search_albums(&self, _query: &str) -> Result<Vec<ProviderSearchAlbum>, ProviderError> {
        Ok(vec![])
    }

    /// Search for tracks by query string.
    /// Default returns empty; providers can override.
    async fn search_tracks(&self, _query: &str) -> Result<Vec<ProviderSearchTrack>, ProviderError> {
        Ok(vec![])
    }
}

/// Provides track download (playback resolution).
#[async_trait]
pub(crate) trait DownloadSource: Send + Sync {
    /// Source identifier.
    fn id(&self) -> Provider;

    /// Whether this source requires provider-linked external IDs.
    fn requires_linked_provider(&self) -> bool {
        true
    }

    /// Resolve playback info (download URL / segments) for a track.
    async fn resolve_playback(
        &self,
        external_track_id: &str,
        quality: &Quality,
        context: Option<&DownloadTrackContext>,
    ) -> Result<PlaybackInfo, ProviderError>;
}

/// Build an image proxy URL for a given provider and image reference.
pub fn provider_image_url(provider: Provider, image_ref: &str, size: u16) -> String {
    format!("/api/image/{provider}/{image_ref}/{size}")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    // ── ProviderError ───────────────────────────────────────────

    #[test]
    fn provider_error_display() {
        let err = ProviderError::invalid_response("test", "something went wrong");
        assert_eq!(
            format!("{err}"),
            "test invalid response: something went wrong"
        );
    }

    #[test]
    fn provider_error_from_string() {
        let err = ProviderError::parse("x", "json", "bad payload");
        assert_eq!(format!("{err}"), "x parse error during json: bad payload");
    }

    #[test]
    fn provider_error_from_owned_string() {
        let err = ProviderError::rate_limited("x", "429");
        assert!(err.is_rate_limited());
    }

    // ── ProviderTrack conversion ────────────────────────────────

    fn base_provider_track() -> ProviderTrack {
        ProviderTrack {
            external_id: "ext-1".to_string(),
            title: "Song Title".to_string(),
            version: Some("Remastered".to_string()),
            track_number: 3,
            disc_number: Some(2),
            duration_secs: 210,
            isrc: Some("USRC12345678".to_string()),
            explicit: true,
            extra: HashMap::new(),
        }
    }

    #[test]
    fn into_track_info_defaults_use_provider_values() {
        let pt = base_provider_track();
        let id = uuid::Uuid::now_v7();
        let info = pt.into_track_info(LocalTrackOverrides {
            id,
            ..Default::default()
        });

        assert_eq!(info.id, id);
        assert_eq!(info.title, "Song Title");
        assert_eq!(info.version.as_deref(), Some("Remastered"));
        assert_eq!(info.disc_number, 2);
        assert_eq!(info.track_number, 3);
        assert_eq!(info.duration_secs, 210);
        assert_eq!(info.isrc.as_deref(), Some("USRC12345678"));
        assert!(info.explicit);
        assert!(info.file_path.is_none());
        assert!(!info.monitored);
        assert!(!info.acquired);
    }

    #[test]
    fn into_track_info_overrides_take_precedence() {
        let pt = base_provider_track();
        let id = uuid::Uuid::now_v7();
        let info = pt.into_track_info(LocalTrackOverrides {
            id,
            disc_number: Some(5),
            track_number: Some(99),
            explicit: Some(false),
            file_path: Some("/music/file.flac".to_string()),
            monitored: true,
            acquired: true,
            quality_override: Some(yoink_shared::Quality::Lossless),
        });

        assert_eq!(info.disc_number, 5);
        assert_eq!(info.track_number, 99);
        assert!(!info.explicit);
        assert_eq!(info.file_path.as_deref(), Some("/music/file.flac"));
        assert!(info.monitored);
        assert!(info.acquired);
        assert_eq!(info.quality_override, Some(yoink_shared::Quality::Lossless));
    }

    #[test]
    fn into_track_info_disc_number_falls_back_to_one() {
        let mut pt = base_provider_track();
        pt.disc_number = None;
        let info = pt.into_track_info(LocalTrackOverrides {
            disc_number: None,
            ..Default::default()
        });
        assert_eq!(info.disc_number, 1);
    }

    #[test]
    fn to_track_info_matches_into_semantics() {
        let pt = base_provider_track();
        let id = uuid::Uuid::now_v7();

        let overrides_a = LocalTrackOverrides {
            id,
            disc_number: Some(7),
            explicit: Some(false),
            ..Default::default()
        };
        let overrides_b = LocalTrackOverrides {
            id,
            disc_number: Some(7),
            explicit: Some(false),
            ..Default::default()
        };

        let via_borrow = pt.to_track_info(overrides_a);
        // Confirm the original is still usable after to_track_info.
        assert_eq!(pt.title, "Song Title");
        let via_move = pt.into_track_info(overrides_b);

        assert_eq!(via_borrow.title, via_move.title);
        assert_eq!(via_borrow.version, via_move.version);
        assert_eq!(via_borrow.disc_number, via_move.disc_number);
        assert_eq!(via_borrow.track_number, via_move.track_number);
        assert_eq!(via_borrow.isrc, via_move.isrc);
        assert_eq!(via_borrow.explicit, via_move.explicit);
        assert_eq!(via_borrow.track_artist, via_move.track_artist);
    }
}
