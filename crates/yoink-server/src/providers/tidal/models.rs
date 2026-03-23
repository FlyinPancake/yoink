//! Serde model types for Tidal / hifi-api JSON responses.
//!
//! These structs mirror the JSON shapes returned by the hifi-api proxy
//! and the uptime discovery feeds. They are intentionally kept close to
//! the wire format; higher-level mapping lives in [`super`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ── Tidal API response types ────────────────────────────────────────

/// Top-level search response wrapper.
#[derive(Debug, Deserialize)]
pub(crate) struct HifiResponse {
    pub data: SearchData,
}

/// Response from the `/artist/` endpoint when fetching an artist's discography.
#[derive(Debug, Deserialize)]
pub(crate) struct HifiArtistAlbumsResponse {
    pub albums: HifiAlbumPage,
}

/// Response from the `/album/` endpoint containing tracks and album metadata.
#[derive(Debug, Deserialize)]
pub(crate) struct HifiAlbumResponse {
    pub data: HifiAlbumData,
}

/// Inner album data: a list of track items plus any extra key-value metadata.
#[derive(Debug, Deserialize)]
pub(crate) struct HifiAlbumData {
    pub items: Vec<HifiAlbumItem>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// A track entry inside an album response.
///
/// The hifi-api returns tracks either wrapped in an `{ "item": ... }` object
/// or directly; this enum handles both shapes via untagged deserialization.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum HifiAlbumItem {
    Item { item: HifiTrack },
    Track(HifiTrack),
}

/// Paginated list of albums (used in artist discography responses).
#[derive(Debug, Deserialize)]
pub(crate) struct HifiAlbumPage {
    pub items: Vec<HifiAlbum>,
}

/// Album metadata as returned by Tidal.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HifiAlbum {
    pub id: i64,
    pub title: String,
    #[serde(rename = "type")]
    pub album_type: Option<String>,
    #[serde(rename = "releaseDate")]
    pub release_date: Option<String>,
    pub cover: Option<String>,
    pub url: Option<String>,
    pub explicit: Option<bool>,
    /// Album-level artists returned by Tidal (main + featured).
    #[serde(default)]
    pub artists: Vec<HifiAlbumArtist>,
}

/// Minimal artist object embedded in album responses.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HifiAlbumArtist {
    pub id: i64,
    pub name: String,
}

/// Track metadata as returned inside album responses.
///
/// Fields not explicitly listed are captured in `extra` via `#[serde(flatten)]`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HifiTrack {
    pub id: i64,
    pub title: String,
    pub version: Option<String>,
    #[serde(rename = "trackNumber")]
    pub track_number: Option<i32>,
    pub duration: Option<i32>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Full artist object as returned by the search endpoint.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HifiArtist {
    pub id: i64,
    pub name: String,
    pub picture: Option<String>,
    #[serde(rename = "selectedAlbumCoverFallback")]
    pub selected_album_cover_fallback: Option<String>,
    pub url: Option<String>,
    pub popularity: Option<u8>,
    #[serde(rename = "artistRoles", default)]
    pub artist_roles: Vec<HifiArtistRole>,
    #[serde(rename = "artistTypes", default)]
    pub artist_types: Vec<String>,
}

/// A single role entry within an artist's `artistRoles` array.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HifiArtistRole {
    pub category: Option<String>,
}

// ── Search wrappers ─────────────────────────────────────────────────

/// The `data` field of a search response, containing optional paged sections.
#[derive(Debug, Deserialize)]
pub(crate) struct SearchData {
    pub artists: Option<PagedArtists>,
    pub albums: Option<PagedAlbums>,
    pub tracks: Option<PagedTracks>,
    pub items: Option<Vec<HifiArtist>>,
}

/// Paginated artist results from search.
#[derive(Debug, Deserialize)]
pub(crate) struct PagedArtists {
    pub items: Vec<HifiArtist>,
}

/// Paginated album results from search.
#[derive(Debug, Deserialize)]
pub(crate) struct PagedAlbums {
    pub items: Vec<HifiAlbum>,
}

/// Paginated track results from search.
#[derive(Debug, Deserialize)]
pub(crate) struct PagedTracks {
    pub items: Vec<HifiSearchTrack>,
}

/// Track result from search (includes album info).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HifiSearchTrack {
    pub id: i64,
    pub title: String,
    pub version: Option<String>,
    pub duration: Option<u32>,
    pub explicit: Option<bool>,
    /// Track-level artists.
    #[serde(default)]
    pub artists: Vec<HifiAlbumArtist>,
    /// Album this track belongs to.
    pub album: Option<HifiSearchTrackAlbum>,
    #[allow(dead_code)]
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Minimal album info embedded in track search results.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HifiSearchTrackAlbum {
    pub id: i64,
    pub title: String,
    pub cover: Option<String>,
}

// ── Playback / manifest ─────────────────────────────────────────────

/// Response from the `/track/` playback endpoint.
#[derive(Debug, Deserialize)]
pub(crate) struct HifiPlaybackResponse {
    pub data: HifiPlaybackData,
}

/// Playback data containing a base64-encoded manifest and its MIME type.
#[derive(Debug, Deserialize)]
pub(crate) struct HifiPlaybackData {
    #[serde(rename = "manifestMimeType")]
    pub manifest_mime_type: String,
    pub manifest: String,
}

/// Decoded BTS manifest (`application/vnd.tidal.bts`) containing direct URLs.
#[derive(Debug, Deserialize)]
pub(crate) struct BtsManifest {
    pub urls: Vec<String>,
}

// ── Instance discovery ──────────────────────────────────────────────

/// A healthy hifi-api instance reported by an uptime feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FeedInstance {
    pub url: String,
    pub version: String,
}

/// An instance reported as down / unhealthy by an uptime feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DownInstance {
    pub url: String,
    pub status: Option<u16>,
    pub error: Option<String>,
}

/// A merged and ranked instance entry ready for failover selection.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RankedInstance {
    pub url: String,
    pub version: String,
    pub source: String,
}

/// JSON shape of an uptime feed response listing API, streaming, and down instances.
#[derive(Debug, Deserialize)]
pub(crate) struct UptimeFeed {
    pub api: Vec<FeedInstance>,
    pub streaming: Vec<FeedInstance>,
    pub down: Vec<DownInstance>,
}
