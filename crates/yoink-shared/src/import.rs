use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

// ── External (manual) import types ──────────────────────────────────

/// How files are integrated into the music library during an external import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManualImportMode {
    /// Create a full independent copy of each file.
    Copy,
    /// Create a hard link (instant, zero extra disk space).
    /// Falls back to copy when source and target are on different filesystems.
    Hardlink,
}

impl ManualImportMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Hardlink => "Hardlink",
        }
    }
}

/// A single entry returned by the server-side path browser.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BrowseEntry {
    /// Display name (file or directory name, not full path).
    pub name: String,
    /// Absolute path on the server.
    pub path: String,
    /// `true` when the entry is a directory.
    pub is_dir: bool,
    /// `true` when the entry is a recognised audio file.
    pub is_audio: bool,
}

/// User-confirmed external import: which source to import, how, and the
/// individual album-level confirmations.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExternalImportConfirmation {
    /// Absolute path on the server that was scanned.
    pub source_path: String,
    /// Copy or hardlink.
    pub mode: ManualImportMode,
    /// Per-album confirmations (same structure as the library-scan import).
    pub items: Vec<ImportConfirmation>,
}

// ── Library-scan import types ───────────────────────────────────────

/// Match quality for a discovered local album during import preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImportMatchStatus {
    /// Exact match found (artist + album title + year all agree).
    Matched,
    /// Artist matched but album only partially matched (fuzzy title or missing year).
    Partial,
    /// No match found in any provider.
    Unmatched,
}

impl ImportMatchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Partial => "partial",
            Self::Unmatched => "unmatched",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Matched => "pill status-completed",
            Self::Partial => "pill status-resolving",
            Self::Unmatched => "pill status-failed",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Matched => "Matched",
            Self::Partial => "Partial Match",
            Self::Unmatched => "Unmatched",
        }
    }
}

/// A candidate album match for a discovered local folder.
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

/// A discovered local album directory with match candidates.
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

/// User-confirmed import selection for a single item.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportConfirmation {
    pub preview_id: String,
    pub artist_name: String,
    pub album_title: String,
    pub year: Option<String>,
    pub artist_id: Option<Uuid>,
    pub album_id: Option<Uuid>,
}

/// Summary of a confirmed import run.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportResultSummary {
    pub total_selected: usize,
    pub imported: usize,
    pub artists_added: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_match_status_as_str() {
        assert_eq!(ImportMatchStatus::Matched.as_str(), "matched");
        assert_eq!(ImportMatchStatus::Partial.as_str(), "partial");
        assert_eq!(ImportMatchStatus::Unmatched.as_str(), "unmatched");
    }

    #[test]
    fn import_match_status_css_class() {
        assert_eq!(
            ImportMatchStatus::Matched.css_class(),
            "pill status-completed"
        );
        assert_eq!(
            ImportMatchStatus::Partial.css_class(),
            "pill status-resolving"
        );
        assert_eq!(
            ImportMatchStatus::Unmatched.css_class(),
            "pill status-failed"
        );
    }

    #[test]
    fn import_match_status_label() {
        assert_eq!(ImportMatchStatus::Matched.label(), "Matched");
        assert_eq!(ImportMatchStatus::Partial.label(), "Partial Match");
        assert_eq!(ImportMatchStatus::Unmatched.label(), "Unmatched");
    }

    #[test]
    fn import_match_status_serde_roundtrip() {
        for status in [
            ImportMatchStatus::Matched,
            ImportMatchStatus::Partial,
            ImportMatchStatus::Unmatched,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: ImportMatchStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn import_match_status_serde_snake_case() {
        let json = serde_json::to_string(&ImportMatchStatus::Matched).unwrap();
        assert_eq!(json, "\"matched\"");
        let json = serde_json::to_string(&ImportMatchStatus::Partial).unwrap();
        assert_eq!(json, "\"partial\"");
        let json = serde_json::to_string(&ImportMatchStatus::Unmatched).unwrap();
        assert_eq!(json, "\"unmatched\"");
    }

    // ── ManualImportMode ────────────────────────────────────────

    #[test]
    fn manual_import_mode_labels() {
        assert_eq!(ManualImportMode::Copy.label(), "Copy");
        assert_eq!(ManualImportMode::Hardlink.label(), "Hardlink");
    }

    #[test]
    fn manual_import_mode_serde_roundtrip() {
        for mode in [ManualImportMode::Copy, ManualImportMode::Hardlink] {
            let json = serde_json::to_string(&mode).unwrap();
            let back: ManualImportMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }

    #[test]
    fn manual_import_mode_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&ManualImportMode::Copy).unwrap(),
            "\"copy\""
        );
        assert_eq!(
            serde_json::to_string(&ManualImportMode::Hardlink).unwrap(),
            "\"hardlink\""
        );
    }
}
