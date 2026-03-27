use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// How files are integrated into the music library during an external import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManualImportMode {
    Copy,
    Hardlink,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BrowseEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExternalImportConfirmation {
    pub source_path: String,
    pub mode: ManualImportMode,
    pub items: Vec<ImportConfirmation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImportMatchStatus {
    Matched,
    Partial,
    Unmatched,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportAlbumCandidate {
    pub album_id: Option<Uuid>,
    pub artist_id: Uuid,
    pub artist_name: String,
    pub album_title: String,
    pub release_date: Option<String>,
    pub cover_url: Option<String>,
    pub album_type: Option<String>,
    pub explicit: bool,
    pub monitored: bool,
    pub acquired: bool,
    pub confidence: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportPreviewItem {
    pub id: String,
    pub relative_path: String,
    pub discovered_artist: String,
    pub discovered_album: String,
    pub discovered_year: Option<String>,
    pub match_status: ImportMatchStatus,
    pub candidates: Vec<ImportAlbumCandidate>,
    pub selected_candidate: Option<usize>,
    pub already_imported: bool,
    pub audio_file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportConfirmation {
    pub preview_id: String,
    pub artist_name: String,
    pub album_title: String,
    pub year: Option<String>,
    pub artist_id: Option<Uuid>,
    pub album_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportResultSummary {
    pub total_selected: usize,
    pub imported: usize,
    pub artists_added: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}
