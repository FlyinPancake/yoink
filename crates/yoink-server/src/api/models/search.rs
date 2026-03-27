use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
    pub artist_name: String,
    pub artist_external_id: String,
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
    pub isrc: Option<String>,
    pub explicit: bool,
    pub artist_name: String,
    pub artist_external_id: String,
    pub album_title: String,
    pub album_external_id: String,
    pub album_cover_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub already_added: Option<bool>,
}
