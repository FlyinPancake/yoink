use std::collections::HashMap;

use uuid::Uuid;

use crate::{Album, DownloadJob, DownloadStatus, MonitoredArtist, TrackInfo};

// ── Well-known fallback strings ─────────────────────────────

pub const UNKNOWN_ARTIST: &str = "Unknown Artist";
pub const UNKNOWN_ALBUM: &str = "Unknown Album";

// ── Display helpers ─────────────────────────────────────────

/// Format a duration in seconds as `"M:SS"` or `"H:MM:SS"` when >= 1 hour.
pub fn format_duration(secs: u32) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let rem = secs % 60;
    if hours > 0 {
        format!("{hours}:{mins:02}:{rem:02}")
    } else {
        format!("{mins}:{rem:02}")
    }
}

#[derive(Debug, Clone)]
pub struct WantedAlbumGroup {
    pub album: Album,
    pub wanted_tracks: Vec<TrackInfo>,
}

#[derive(Debug, Clone)]
pub struct WantedArtistGroup {
    pub artist_id: Uuid,
    pub artist_name: String,
    pub albums: Vec<WantedAlbumGroup>,
}

// ── Data helpers (pure transforms) ──────────────────────────

/// For each album_id, keep only the most recently updated job.
pub fn build_latest_jobs(jobs: Vec<DownloadJob>) -> HashMap<Uuid, DownloadJob> {
    let mut map: HashMap<Uuid, DownloadJob> = HashMap::new();
    for job in jobs {
        map.entry(job.album_id)
            .and_modify(|existing| {
                if job.updated_at > existing.updated_at {
                    *existing = job.clone();
                }
            })
            .or_insert(job);
    }
    map
}

/// Map artist id -> name for display.
pub fn build_artist_names(artists: &[MonitoredArtist]) -> HashMap<Uuid, String> {
    artists.iter().map(|a| (a.id, a.name.clone())).collect()
}

// ── Display helpers ─────────────────────────────────────────

pub fn status_label_text(status: &DownloadStatus, completed: usize, total: usize) -> String {
    match status {
        DownloadStatus::Queued => "Queued".to_string(),
        DownloadStatus::Resolving => "Resolving".to_string(),
        DownloadStatus::Downloading => {
            if total > 0 {
                format!("Downloading {completed}/{total}")
            } else {
                "Downloading".to_string()
            }
        }
        DownloadStatus::Completed => "Completed".to_string(),
        DownloadStatus::Failed => "Failed".to_string(),
    }
}

pub fn status_class(status: &DownloadStatus) -> &'static str {
    match status {
        DownloadStatus::Queued => "pill status-queued",
        DownloadStatus::Resolving => "pill status-resolving",
        DownloadStatus::Downloading => "pill status-downloading",
        DownloadStatus::Completed => "pill status-completed",
        DownloadStatus::Failed => "pill status-failed",
    }
}

pub fn album_type_label(album_type: Option<&str>, title: &str) -> &'static str {
    if let Some(kind) = album_type {
        let k = kind.to_ascii_lowercase();
        if k.contains("ep") {
            return "EP";
        }
        if k.contains("single") {
            return "Single";
        }
        if k.contains("album") {
            return "Album";
        }
    }
    let t = title.to_ascii_lowercase();
    if t.contains(" ep") || t.ends_with("ep") || t.contains("(ep") {
        return "EP";
    }
    if t.contains(" single") || t.ends_with("single") || t.contains("(single") {
        return "Single";
    }
    "Album"
}

pub fn album_type_rank(album_type: Option<&str>, title: &str) -> u8 {
    match album_type_label(album_type, title) {
        "Album" => 0,
        "EP" => 1,
        "Single" => 2,
        _ => 3,
    }
}

/// Human-readable display name for a provider ID.
pub fn provider_display_name(provider: &str) -> String {
    match provider {
        "tidal" => "Tidal".to_string(),
        "musicbrainz" => "MusicBrainz".to_string(),
        "deezer" => "Deezer".to_string(),
        "soulseek" => "SoulSeek".to_string(),
        other => {
            let mut c = other.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::{DownloadJob, DownloadStatus, MonitoredArtist, TrackInfo};

    // ── Helper factories ────────────────────────────────────────

    fn make_artist(name: &str) -> MonitoredArtist {
        MonitoredArtist {
            id: Uuid::now_v7(),
            name: name.to_string(),
            image_url: None,
            bio: None,
            monitored: true,
            created_at: Utc::now(),
        }
    }

    fn make_album(artist_id: Uuid, title: &str, release_date: Option<&str>) -> Album {
        Album {
            id: Uuid::now_v7(),
            title: title.to_string(),
            album_type: None,
            release_date: release_date.map(|s| s.to_string()),
            cover_url: None,
            explicit: false,
            monitored: false,
            wanted_status: todo!(),
            created_at: todo!(),
        }
    }

    fn make_job(album_id: Uuid, updated_at: chrono::DateTime<Utc>) -> DownloadJob {
        DownloadJob {
            id: Uuid::now_v7(),
            album_id,
            source: "tidal".to_string(),
            album_title: "Test Album".to_string(),
            artist_name: "Test Artist".to_string(),
            status: DownloadStatus::Completed,
            quality: crate::Quality::Lossless,
            total_tracks: 10,
            completed_tracks: 10,
            error: None,
            created_at: updated_at,
            updated_at,
        }
    }

    fn make_track(title: &str, monitored: bool, acquired: bool) -> TrackInfo {
        TrackInfo {
            id: Uuid::now_v7(),
            title: title.to_string(),
            version: None,
            disc_number: 1,
            track_number: 1,
            duration_secs: 200,
            isrc: None,
            explicit: false,
            quality_override: None,
            track_artist: None,
            file_path: None,
            monitored,
            acquired,
        }
    }

    // ── build_latest_jobs ───────────────────────────────────────

    #[test]
    fn build_latest_jobs_empty() {
        let result = build_latest_jobs(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn build_latest_jobs_single_per_album() {
        let album_id = Uuid::now_v7();
        let job = make_job(album_id, Utc::now());
        let result = build_latest_jobs(vec![job.clone()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[&album_id].id, job.id);
    }

    #[test]
    fn build_latest_jobs_keeps_newest() {
        let album_id = Uuid::now_v7();
        let old = make_job(album_id, Utc::now() - chrono::Duration::hours(2));
        let new = make_job(album_id, Utc::now());
        let new_id = new.id;
        // Insert old first, then new
        let result = build_latest_jobs(vec![old, new]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[&album_id].id, new_id);
    }

    #[test]
    fn build_latest_jobs_keeps_newest_reverse_order() {
        let album_id = Uuid::now_v7();
        let old = make_job(album_id, Utc::now() - chrono::Duration::hours(2));
        let new = make_job(album_id, Utc::now());
        let new_id = new.id;
        // Insert new first, then old — should still keep new
        let result = build_latest_jobs(vec![new, old]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[&album_id].id, new_id);
    }

    // ── status_label_text ───────────────────────────────────────

    #[test]
    fn status_label_queued() {
        assert_eq!(status_label_text(&DownloadStatus::Queued, 0, 0), "Queued");
    }

    #[test]
    fn status_label_resolving() {
        assert_eq!(
            status_label_text(&DownloadStatus::Resolving, 0, 0),
            "Resolving"
        );
    }

    #[test]
    fn status_label_downloading_with_total() {
        assert_eq!(
            status_label_text(&DownloadStatus::Downloading, 3, 10),
            "Downloading 3/10"
        );
    }

    #[test]
    fn status_label_downloading_no_total() {
        assert_eq!(
            status_label_text(&DownloadStatus::Downloading, 0, 0),
            "Downloading"
        );
    }

    #[test]
    fn status_label_completed() {
        assert_eq!(
            status_label_text(&DownloadStatus::Completed, 10, 10),
            "Completed"
        );
    }

    #[test]
    fn status_label_failed() {
        assert_eq!(status_label_text(&DownloadStatus::Failed, 0, 0), "Failed");
    }

    // ── status_class ────────────────────────────────────────────

    #[test]
    fn status_class_all_variants() {
        assert_eq!(status_class(&DownloadStatus::Queued), "pill status-queued");
        assert_eq!(
            status_class(&DownloadStatus::Resolving),
            "pill status-resolving"
        );
        assert_eq!(
            status_class(&DownloadStatus::Downloading),
            "pill status-downloading"
        );
        assert_eq!(
            status_class(&DownloadStatus::Completed),
            "pill status-completed"
        );
        assert_eq!(status_class(&DownloadStatus::Failed), "pill status-failed");
    }

    // ── album_type_label ────────────────────────────────────────

    #[test]
    fn album_type_label_from_explicit_type() {
        assert_eq!(album_type_label(Some("EP"), "Some Title"), "EP");
        assert_eq!(album_type_label(Some("single"), "Some Title"), "Single");
        assert_eq!(album_type_label(Some("Album"), "Some Title"), "Album");
    }

    #[test]
    fn album_type_label_from_title_heuristics() {
        assert_eq!(album_type_label(None, "My Album EP"), "EP");
        assert_eq!(album_type_label(None, "My Album (EP)"), "EP");
        assert_eq!(album_type_label(None, "Sunrise single"), "Single");
        assert_eq!(album_type_label(None, "Sunrise (single)"), "Single");
    }

    #[test]
    fn album_type_label_fallback_to_album() {
        assert_eq!(album_type_label(None, "Regular Title"), "Album");
        assert_eq!(album_type_label(Some("compilation"), "Title"), "Album");
    }

    // ── album_type_rank ─────────────────────────────────────────

    #[test]
    fn album_type_rank_ordering() {
        assert_eq!(album_type_rank(Some("Album"), ""), 0);
        assert_eq!(album_type_rank(Some("EP"), ""), 1);
        assert_eq!(album_type_rank(Some("Single"), ""), 2);
        // Album < EP < Single
        assert!(album_type_rank(Some("Album"), "") < album_type_rank(Some("EP"), ""));
        assert!(album_type_rank(Some("EP"), "") < album_type_rank(Some("Single"), ""));
    }

    // ── provider_display_name ───────────────────────────────────

    #[test]
    fn provider_display_name_known() {
        assert_eq!(provider_display_name("tidal"), "Tidal");
        assert_eq!(provider_display_name("musicbrainz"), "MusicBrainz");
        assert_eq!(provider_display_name("deezer"), "Deezer");
        assert_eq!(provider_display_name("soulseek"), "SoulSeek");
    }

    #[test]
    fn provider_display_name_unknown_capitalizes() {
        assert_eq!(provider_display_name("spotify"), "Spotify");
        assert_eq!(provider_display_name("bandcamp"), "Bandcamp");
    }

    #[test]
    fn provider_display_name_empty_string() {
        assert_eq!(provider_display_name(""), "");
    }
}
