use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db;

use super::Quality;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrackInfo {
    pub id: Uuid,
    pub title: String,
    pub version: Option<String>,
    pub disc_number: i32,
    pub track_number: i32,
    pub duration_secs: i32,
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

impl From<db::track::Model> for TrackInfo {
    fn from(value: db::track::Model) -> Self {
        let acquired =
            value.file_path.is_some() || value.status == db::wanted_status::WantedStatus::Acquired;

        TrackInfo {
            id: value.id,
            title: value.title,
            version: value.version,
            disc_number: value.disc_number.unwrap_or(1),
            track_number: value.track_number.unwrap_or(1),
            duration_secs: value.duration.unwrap_or_default(),
            isrc: value.isrc,
            explicit: value.explicit,
            file_path: value.file_path,
            monitored: value.status != db::wanted_status::WantedStatus::Unmonitored,
            acquired,
            quality_override: value.quality_override,
            track_artist: None,
        }
    }
}

impl From<db::track::ModelEx> for TrackInfo {
    fn from(value: db::track::ModelEx) -> Self {
        db::track::Model::from(value).into()
    }
}
