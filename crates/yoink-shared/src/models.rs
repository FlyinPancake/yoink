use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::ParseQualityError;

/// Normalized quality level, provider-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum Quality {
    /// Hi-res lossless (up to FLAC 24-bit/192kHz).
    HiRes,
    /// Standard lossless (FLAC 16-bit/44.1kHz).
    Lossless,
    /// High-quality lossy (e.g. 320kbps MP3/AAC).
    High,
    /// Low-quality lossy (e.g. 96 ~ 128kbps MP3/AAC).
    Low,
}

impl Quality {
    pub fn as_str(&self) -> &str {
        match self {
            Quality::Lossless => "LOSSLESS",
            Quality::HiRes => "HI_RES_LOSSLESS",
            Quality::High => "HIGH",
            Quality::Low => "LOW",
        }
    }
}

impl std::fmt::Display for Quality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Quality {
    type Err = ParseQualityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "HI_RES_LOSSLESS" | "HIRES_LOSSLESS" | "HI_RES" | "HIRES" => Ok(Quality::HiRes),
            "LOSSLESS" => Ok(Quality::Lossless),
            "HIGH" => Ok(Quality::High),
            "LOW" => Ok(Quality::Low),
            _ => Err(ParseQualityError {
                value: s.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Queued,
    Resolving,
    Downloading,
    Completed,
    Failed,
}

impl DownloadStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Resolving => "resolving",
            Self::Downloading => "downloading",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DownloadJob {
    pub id: Uuid,
    pub album_id: Uuid,
    pub source: String,
    pub album_title: String,
    pub artist_name: String,
    pub status: DownloadStatus,
    pub quality: Quality,
    pub total_tracks: usize,
    pub completed_tracks: usize,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MonitoredArtist {
    pub id: Uuid,
    pub name: String,
    pub image_url: Option<String>,
    pub bio: Option<String>,
    /// Whether this artist is fully monitored (discography synced from providers).
    /// `false` = lightweight artist (only explicitly-added albums, no auto-sync).
    pub monitored: bool,
    pub added_at: DateTime<Utc>,
}

/// A raw artist credit from a provider, stored on the album.
/// Used to display all album artists even when some aren't monitored locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ArtistCredit {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MonitoredAlbum {
    pub id: Uuid,
    /// Primary (first) artist — kept for backward compatibility and as a
    /// convenient shorthand for the common single-artist case.
    pub artist_id: Uuid,
    /// All artists associated with this album, ordered by display priority.
    /// The first entry always equals `artist_id`.
    #[serde(default)]
    pub artist_ids: Vec<Uuid>,
    /// Raw artist credits from providers. Includes artists that may not be
    /// monitored locally. Used for display on the album detail page.
    #[serde(default)]
    pub artist_credits: Vec<ArtistCredit>,
    pub title: String,
    pub album_type: Option<String>,
    pub release_date: Option<String>,
    pub cover_url: Option<String>,
    pub explicit: bool,
    #[serde(default)]
    pub quality_override: Option<Quality>,
    pub monitored: bool,
    pub acquired: bool,
    pub wanted: bool,
    /// True when the album is not fully monitored but has individually monitored
    /// tracks that are not yet acquired.
    #[serde(default)]
    pub partially_wanted: bool,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrackInfo {
    pub id: Uuid,
    pub title: String,
    pub version: Option<String>,
    pub disc_number: u32,
    pub track_number: u32,
    pub duration_secs: u32,
    pub duration_display: String,
    pub isrc: Option<String>,
    pub explicit: bool,
    #[serde(default)]
    pub quality_override: Option<Quality>,
    /// Track-level artist string (may differ from album artist for features/collabs).
    pub track_artist: Option<String>,
    /// Local file path relative to the music root (populated for acquired albums).
    pub file_path: Option<String>,
    /// Whether this individual track is monitored for download.
    pub monitored: bool,
    /// Whether this track has been acquired (file exists on disk).
    pub acquired: bool,
}

/// A track with its parent album and artist context, for library-wide views.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LibraryTrack {
    pub track: TrackInfo,
    pub album_id: Uuid,
    pub album_title: String,
    pub album_cover_url: Option<String>,
    pub artist_id: Uuid,
    pub artist_name: String,
}

/// Provider link info for the UI.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderLink {
    pub provider: String,
    pub external_id: String,
    pub external_url: Option<String>,
    pub external_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthStatus {
    pub auth_enabled: bool,
    pub authenticated: bool,
    pub username: Option<String>,
    pub must_change_password: bool,
}

/// Potential cross-provider match suggestion.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MatchSuggestion {
    pub id: Uuid,
    pub scope_type: String,
    pub scope_id: Uuid,
    pub left_provider: String,
    pub left_external_id: String,
    pub right_provider: String,
    pub right_external_id: String,
    pub match_kind: String,
    pub confidence: u8,
    pub explanation: Option<String>,
    pub external_name: Option<String>,
    pub external_url: Option<String>,
    pub image_url: Option<String>,
    pub disambiguation: Option<String>,
    pub artist_type: Option<String>,
    pub country: Option<String>,
    pub tags: Vec<String>,
    pub popularity: Option<u8>,
    pub status: String,
}

/// An artist image option from a linked provider.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArtistImageOption {
    pub provider: String,
    pub image_url: String,
}

/// A search result from a metadata provider.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchArtistResult {
    pub provider: String,
    pub external_id: String,
    pub name: String,
    pub image_url: Option<String>,
    pub url: Option<String>,
    pub disambiguation: Option<String>,
    pub artist_type: Option<String>,
    pub country: Option<String>,
    pub tags: Vec<String>,
    pub popularity: Option<u8>,
    /// `Some(true)` when the artist is already in the library.
    /// Only populated by server-side search handlers; defaults to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub already_monitored: Option<bool>,
}

/// An album search result from a metadata provider.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchAlbumResult {
    pub provider: String,
    pub external_id: String,
    pub title: String,
    pub album_type: Option<String>,
    pub release_date: Option<String>,
    pub cover_url: Option<String>,
    pub url: Option<String>,
    pub explicit: bool,
    /// Primary artist name for display.
    pub artist_name: String,
    /// Provider-specific external ID for the primary artist.
    pub artist_external_id: String,
    /// `Some(true)` when the album is already in the library.
    /// Only populated by server-side search handlers; defaults to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub already_added: Option<bool>,
}

/// A track search result from a metadata provider.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchTrackResult {
    pub provider: String,
    pub external_id: String,
    pub title: String,
    pub version: Option<String>,
    pub duration_secs: u32,
    pub duration_display: String,
    pub isrc: Option<String>,
    pub explicit: bool,
    /// Display-ready track artist string.
    pub artist_name: String,
    pub artist_external_id: String,
    /// Album info for context.
    pub album_title: String,
    pub album_external_id: String,
    pub album_cover_url: Option<String>,
    /// `Some(true)` when the track is already in the library.
    /// Only populated by server-side search handlers; defaults to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub already_added: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Quality ─────────────────────────────────────────────────

    #[test]
    fn quality_env_parser_handles_aliases_and_fallback() {
        let q: Quality = "hires".parse().expect("should parse");
        assert_eq!(q, Quality::HiRes);

        let q: Quality = "HI_RES_LOSSLESS".parse().expect("should parse");
        assert_eq!(q, Quality::HiRes);

        let q: Quality = "  lossless  "
            .parse()
            .expect("should parse with whitespace");
        assert_eq!(q, Quality::Lossless);

        let q: Result<Quality, ParseQualityError> = "definitely-not-a-quality".parse();
        assert!(q.is_err())
    }

    #[test]
    fn quality_as_str_all_variants() {
        assert_eq!(Quality::HiRes.as_str(), "HI_RES_LOSSLESS");
        assert_eq!(Quality::Lossless.as_str(), "LOSSLESS");
        assert_eq!(Quality::High.as_str(), "HIGH");
        assert_eq!(Quality::Low.as_str(), "LOW");
    }

    #[test]
    fn quality_display_matches_as_str() {
        for q in [
            Quality::HiRes,
            Quality::Lossless,
            Quality::High,
            Quality::Low,
        ] {
            assert_eq!(format!("{q}"), q.as_str());
        }
    }

    #[test]
    fn quality_from_str_all_canonical() {
        assert_eq!("HIGH".parse::<Quality>().unwrap(), Quality::High);
        assert_eq!("LOW".parse::<Quality>().unwrap(), Quality::Low);
        assert_eq!("LOSSLESS".parse::<Quality>().unwrap(), Quality::Lossless);
        assert_eq!("HIRES_LOSSLESS".parse::<Quality>().unwrap(), Quality::HiRes);
    }

    #[test]
    fn quality_serde_roundtrip() {
        for q in [
            Quality::HiRes,
            Quality::Lossless,
            Quality::High,
            Quality::Low,
        ] {
            let json = serde_json::to_string(&q).unwrap();
            let back: Quality = serde_json::from_str(&json).unwrap();
            assert_eq!(q, back);
        }
    }

    // ── DownloadStatus ──────────────────────────────────────────

    #[test]
    fn download_status_as_str() {
        assert_eq!(DownloadStatus::Queued.as_str(), "queued");
        assert_eq!(DownloadStatus::Resolving.as_str(), "resolving");
        assert_eq!(DownloadStatus::Downloading.as_str(), "downloading");
        assert_eq!(DownloadStatus::Completed.as_str(), "completed");
        assert_eq!(DownloadStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn download_status_serde_roundtrip() {
        for status in [
            DownloadStatus::Queued,
            DownloadStatus::Resolving,
            DownloadStatus::Downloading,
            DownloadStatus::Completed,
            DownloadStatus::Failed,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: DownloadStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn download_status_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&DownloadStatus::Queued).unwrap(),
            "\"queued\""
        );
        assert_eq!(
            serde_json::to_string(&DownloadStatus::Downloading).unwrap(),
            "\"downloading\""
        );
    }

    // ── ArtistCredit serde ──────────────────────────────────────

    #[test]
    fn artist_credit_skips_none_fields() {
        let credit = ArtistCredit {
            name: "Artist".to_string(),
            provider: None,
            external_id: None,
        };
        let json = serde_json::to_string(&credit).unwrap();
        assert!(!json.contains("provider"));
        assert!(!json.contains("external_id"));
    }

    #[test]
    fn artist_credit_includes_some_fields() {
        let credit = ArtistCredit {
            name: "Artist".to_string(),
            provider: Some("tidal".to_string()),
            external_id: Some("123".to_string()),
        };
        let json = serde_json::to_string(&credit).unwrap();
        assert!(json.contains("\"provider\":\"tidal\""));
        assert!(json.contains("\"external_id\":\"123\""));
    }

    #[test]
    fn artist_credit_deserializes_missing_optional_fields() {
        let json = r#"{"name":"Artist"}"#;
        let credit: ArtistCredit = serde_json::from_str(json).unwrap();
        assert_eq!(credit.name, "Artist");
        assert_eq!(credit.provider, None);
        assert_eq!(credit.external_id, None);
    }

    // ── MonitoredAlbum serde ────────────────────────────────────

    #[test]
    fn monitored_album_partially_wanted_defaults_to_false() {
        // Simulate JSON from an older version that doesn't have partially_wanted
        let json = serde_json::json!({
            "id": "01933e10-b4a4-7000-8000-000000000001",
            "artist_id": "01933e10-b4a4-7000-8000-000000000002",
            "title": "Test",
            "explicit": false,
            "monitored": false,
            "acquired": false,
            "wanted": false,
            "added_at": "2024-01-01T00:00:00Z"
        });
        let album: MonitoredAlbum = serde_json::from_value(json).unwrap();
        assert!(!album.partially_wanted);
        assert!(album.artist_ids.is_empty());
        assert!(album.artist_credits.is_empty());
        assert_eq!(album.quality_override, None);
    }

    #[test]
    fn track_info_quality_override_defaults_to_none() {
        let json = serde_json::json!({
            "id": "01933e10-b4a4-7000-8000-000000000010",
            "title": "Track",
            "version": null,
            "disc_number": 1,
            "track_number": 1,
            "duration_secs": 180,
            "duration_display": "3:00",
            "isrc": null,
            "explicit": false,
            "track_artist": null,
            "file_path": null,
            "monitored": false,
            "acquired": false
        });

        let track: TrackInfo = serde_json::from_value(json).unwrap();
        assert_eq!(track.quality_override, None);
    }

    // ── SearchArtistResult serde ─────────────────────────────────

    #[test]
    fn already_monitored_defaults_to_none_when_missing() {
        let json = serde_json::json!({
            "provider": "tidal",
            "external_id": "12345",
            "name": "Test Artist",
            "tags": [],
        });
        let result: SearchArtistResult = serde_json::from_value(json).unwrap();
        assert_eq!(result.already_monitored, None);
    }

    #[test]
    fn already_monitored_none_is_omitted_on_serialize() {
        let result = SearchArtistResult {
            provider: "tidal".to_string(),
            external_id: "12345".to_string(),
            name: "Test Artist".to_string(),
            image_url: None,
            url: None,
            disambiguation: None,
            artist_type: None,
            country: None,
            tags: vec![],
            popularity: None,
            already_monitored: None,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert!(
            !json.as_object().unwrap().contains_key("already_monitored"),
            "already_monitored: None should be skipped in serialized JSON"
        );
    }
}
