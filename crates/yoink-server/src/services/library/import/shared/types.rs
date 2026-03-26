use std::path::PathBuf;

use crate::db::{album, artist};

pub(super) const ALBUM_CANDIDATE_MIN_CONFIDENCE: u8 = 65;
pub(super) const ARTIST_CANDIDATE_MIN_CONFIDENCE: u8 = 72;
pub(super) const MATCHED_CONFIDENCE: u8 = 90;
pub(super) const EXDEV_ERROR_CODE: i32 = 18;
pub(super) const STRONG_FUZZY_MATCH_SCORE: f64 = 0.92;

#[derive(Debug, Clone, Default)]
pub(super) struct EmbeddedTrackMetadata {
    pub(super) album_artist: Option<String>,
    pub(super) track_artist: Option<String>,
    pub(super) album_title: Option<String>,
    pub(super) track_title: Option<String>,
    pub(super) year: Option<String>,
    pub(super) disc_number: Option<i32>,
    pub(super) track_number: Option<i32>,
    pub(super) duration_secs: Option<i32>,
    pub(super) isrc: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ScannedAudioFile {
    pub(super) absolute_path: PathBuf,
    pub(super) embedded: EmbeddedTrackMetadata,
}

#[derive(Debug, Clone)]
pub(super) struct DiscoveredAlbum {
    pub(super) id: String,
    pub(super) relative_path: String,
    pub(super) discovered_artist: String,
    pub(super) discovered_album: String,
    pub(super) discovered_year: Option<String>,
    pub(super) files: Vec<ScannedAudioFile>,
}

#[derive(Debug, Clone)]
pub(super) struct PreparedTrack {
    pub(super) source_path: PathBuf,
    pub(super) title: String,
    pub(super) disc_number: Option<i32>,
    pub(super) track_number: Option<i32>,
    pub(super) duration_secs: Option<i32>,
    pub(super) isrc: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct LocalArtistCatalog {
    pub(super) artist: artist::Model,
    pub(super) albums: Vec<album::Model>,
}
