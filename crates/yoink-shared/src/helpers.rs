use std::collections::HashMap;

use uuid::Uuid;

use crate::{DownloadJob, DownloadStatus, MonitoredAlbum, MonitoredArtist, TrackInfo};

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
    pub album: MonitoredAlbum,
    pub wanted_tracks: Vec<TrackInfo>,
}

#[derive(Debug, Clone)]
pub struct WantedArtistGroup {
    pub artist_id: Uuid,
    pub artist_name: String,
    pub albums: Vec<WantedAlbumGroup>,
}

// ── Data helpers (pure transforms) ──────────────────────────

/// Group albums by artist_id, sorted newest-first within each group.
/// Albums with multiple artists appear under each artist's group.
pub fn build_albums_by_artist(albums: Vec<MonitoredAlbum>) -> HashMap<Uuid, Vec<MonitoredAlbum>> {
    let mut map: HashMap<Uuid, Vec<MonitoredAlbum>> = HashMap::new();
    for album in albums {
        if album.artist_ids.is_empty() {
            // Fallback: use the primary artist_id
            map.entry(album.artist_id).or_default().push(album);
        } else {
            for &aid in &album.artist_ids {
                map.entry(aid).or_default().push(album.clone());
            }
        }
    }
    for albums in map.values_mut() {
        albums.sort_by(|a, b| {
            b.release_date
                .cmp(&a.release_date)
                .then_with(|| a.title.cmp(&b.title))
        });
    }
    map
}

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

/// Build hierarchical wanted data: artist > album > wanted tracks.
///
/// - Includes albums that are fully wanted (`album.wanted`) or partially wanted.
/// - For fully monitored albums, `wanted_tracks` is empty (UI can show "full album").
/// - For partially wanted albums, `wanted_tracks` includes tracks where
///   `track.monitored && !track.acquired`.
pub fn build_wanted_tree(
    artists: &[MonitoredArtist],
    albums_with_tracks: Vec<(MonitoredAlbum, Vec<TrackInfo>)>,
) -> Vec<WantedArtistGroup> {
    let names = build_artist_names(artists);
    let mut by_artist: HashMap<Uuid, Vec<WantedAlbumGroup>> = HashMap::new();

    for (album, tracks) in albums_with_tracks {
        if !(album.wanted || album.partially_wanted) {
            continue;
        }

        let wanted_tracks = if album.monitored {
            Vec::new()
        } else {
            tracks
                .into_iter()
                .filter(|t| t.monitored && !t.acquired)
                .collect::<Vec<_>>()
        };

        if !album.monitored && wanted_tracks.is_empty() {
            continue;
        }

        by_artist
            .entry(album.artist_id)
            .or_default()
            .push(WantedAlbumGroup {
                album,
                wanted_tracks,
            });
    }

    for albums in by_artist.values_mut() {
        albums.sort_by(|a, b| {
            b.album
                .release_date
                .cmp(&a.album.release_date)
                .then_with(|| a.album.title.cmp(&b.album.title))
        });
    }

    let mut groups = by_artist
        .into_iter()
        .map(|(artist_id, albums)| WantedArtistGroup {
            artist_id,
            artist_name: names
                .get(&artist_id)
                .cloned()
                .unwrap_or_else(|| format!("Unknown ({artist_id})")),
            albums,
        })
        .collect::<Vec<_>>();

    groups.sort_by(|a, b| {
        a.artist_name
            .to_lowercase()
            .cmp(&b.artist_name.to_lowercase())
    });
    groups
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

// ── Asset/URL helpers ───────────────────────────────────────

/// Build an image proxy URL for a given provider and image reference.
pub fn provider_image_url(provider: &str, image_ref: &str, size: u16) -> String {
    format!("/api/image/{provider}/{image_ref}/{size}")
}

/// Get the cover URL for an album (already a full URL or None).
pub fn album_cover_url(album: &MonitoredAlbum, _size: u16) -> Option<String> {
    album.cover_url.clone()
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
    use crate::{DownloadJob, DownloadStatus, MonitoredAlbum, MonitoredArtist, TrackInfo};

    // ── Helper factories ────────────────────────────────────────

    fn make_artist(name: &str) -> MonitoredArtist {
        MonitoredArtist {
            id: Uuid::now_v7(),
            name: name.to_string(),
            image_url: None,
            bio: None,
            monitored: true,
            added_at: Utc::now(),
        }
    }

    fn make_album(artist_id: Uuid, title: &str, release_date: Option<&str>) -> MonitoredAlbum {
        MonitoredAlbum {
            id: Uuid::now_v7(),
            artist_id,
            artist_ids: vec![artist_id],
            artist_credits: Vec::new(),
            title: title.to_string(),
            album_type: None,
            release_date: release_date.map(|s| s.to_string()),
            cover_url: None,
            explicit: false,
            quality_override: None,
            monitored: false,
            acquired: false,
            wanted: false,
            partially_wanted: false,
            added_at: Utc::now(),
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
            duration_display: "3:20".to_string(),
            isrc: None,
            explicit: false,
            quality_override: None,
            track_artist: None,
            file_path: None,
            monitored,
            acquired,
        }
    }

    // ── build_albums_by_artist ──────────────────────────────────

    #[test]
    fn build_albums_by_artist_empty() {
        let result = build_albums_by_artist(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn build_albums_by_artist_single_artist() {
        let artist_id = Uuid::now_v7();
        let a1 = make_album(artist_id, "Album A", Some("2023-01-01"));
        let a2 = make_album(artist_id, "Album B", Some("2024-06-15"));
        let result = build_albums_by_artist(vec![a1, a2]);
        assert_eq!(result.len(), 1);
        let albums = &result[&artist_id];
        assert_eq!(albums.len(), 2);
        // Sorted newest-first
        assert_eq!(albums[0].title, "Album B");
        assert_eq!(albums[1].title, "Album A");
    }

    #[test]
    fn build_albums_by_artist_multi_artist_album_duplicated() {
        let artist_a = Uuid::now_v7();
        let artist_b = Uuid::now_v7();
        let mut album = make_album(artist_a, "Collab Album", Some("2024-01-01"));
        album.artist_ids = vec![artist_a, artist_b];
        let result = build_albums_by_artist(vec![album]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[&artist_a].len(), 1);
        assert_eq!(result[&artist_b].len(), 1);
        assert_eq!(result[&artist_a][0].title, "Collab Album");
        assert_eq!(result[&artist_b][0].title, "Collab Album");
    }

    #[test]
    fn build_albums_by_artist_empty_artist_ids_uses_fallback() {
        let artist_id = Uuid::now_v7();
        let mut album = make_album(artist_id, "Solo Album", Some("2024-01-01"));
        album.artist_ids = vec![];
        let result = build_albums_by_artist(vec![album]);
        assert_eq!(result.len(), 1);
        assert!(result.contains_key(&artist_id));
    }

    #[test]
    fn build_albums_by_artist_sorts_by_date_then_title() {
        let artist_id = Uuid::now_v7();
        let a1 = make_album(artist_id, "Zebra", Some("2024-01-01"));
        let a2 = make_album(artist_id, "Alpha", Some("2024-01-01"));
        let a3 = make_album(artist_id, "Middle", Some("2023-01-01"));
        let result = build_albums_by_artist(vec![a1, a2, a3]);
        let albums = &result[&artist_id];
        // Same date: sorted alphabetically by title
        assert_eq!(albums[0].title, "Alpha");
        assert_eq!(albums[1].title, "Zebra");
        // Older date comes last
        assert_eq!(albums[2].title, "Middle");
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

    // ── build_wanted_tree ───────────────────────────────────────

    #[test]
    fn build_wanted_tree_empty() {
        let result = build_wanted_tree(&[], vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn build_wanted_tree_skips_unwanted_albums() {
        let artist = make_artist("Artist");
        let mut album = make_album(artist.id, "Unwanted", Some("2024-01-01"));
        album.wanted = false;
        album.partially_wanted = false;
        let tracks = vec![make_track("Track 1", false, false)];
        let result = build_wanted_tree(&[artist], vec![(album, tracks)]);
        assert!(result.is_empty());
    }

    #[test]
    fn build_wanted_tree_fully_wanted_album_has_empty_tracks() {
        let artist = make_artist("Artist");
        let mut album = make_album(artist.id, "Wanted Album", Some("2024-01-01"));
        album.wanted = true;
        album.monitored = true;
        let tracks = vec![
            make_track("Track 1", true, false),
            make_track("Track 2", true, true),
        ];
        let result = build_wanted_tree(&[artist], vec![(album, tracks)]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].albums.len(), 1);
        // Fully monitored album: wanted_tracks should be empty
        assert!(result[0].albums[0].wanted_tracks.is_empty());
    }

    #[test]
    fn build_wanted_tree_partially_wanted_filters_tracks() {
        let artist = make_artist("Artist");
        let mut album = make_album(artist.id, "Partial Album", Some("2024-01-01"));
        album.partially_wanted = true;
        album.monitored = false;
        let tracks = vec![
            make_track("Track 1", true, false), // wanted: monitored + not acquired
            make_track("Track 2", true, true),  // not wanted: already acquired
            make_track("Track 3", false, false), // not wanted: not monitored
        ];
        let result = build_wanted_tree(&[artist], vec![(album, tracks)]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].albums[0].wanted_tracks.len(), 1);
        assert_eq!(result[0].albums[0].wanted_tracks[0].title, "Track 1");
    }

    #[test]
    fn build_wanted_tree_partially_wanted_but_no_tracks_skipped() {
        let artist = make_artist("Artist");
        let mut album = make_album(artist.id, "No Tracks Album", Some("2024-01-01"));
        album.partially_wanted = true;
        album.monitored = false;
        // All tracks acquired, so none are wanted
        let tracks = vec![make_track("Track 1", true, true)];
        let result = build_wanted_tree(&[artist], vec![(album, tracks)]);
        assert!(result.is_empty());
    }

    #[test]
    fn build_wanted_tree_sorted_by_artist_name() {
        let artist_z = make_artist("Zebra");
        let artist_a = make_artist("Alpha");
        let mut album_z = make_album(artist_z.id, "Z Album", Some("2024-01-01"));
        album_z.wanted = true;
        album_z.monitored = true;
        let mut album_a = make_album(artist_a.id, "A Album", Some("2024-01-01"));
        album_a.wanted = true;
        album_a.monitored = true;
        let result = build_wanted_tree(
            &[artist_z, artist_a],
            vec![(album_z, vec![]), (album_a, vec![])],
        );
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].artist_name, "Alpha");
        assert_eq!(result[1].artist_name, "Zebra");
    }

    #[test]
    fn build_wanted_tree_unknown_artist_fallback() {
        let fake_artist_id = Uuid::now_v7();
        let mut album = make_album(fake_artist_id, "Orphan Album", Some("2024-01-01"));
        album.wanted = true;
        album.monitored = true;
        // No artist provided — name should fall back to "Unknown (uuid)"
        let result = build_wanted_tree(&[], vec![(album, vec![])]);
        assert_eq!(result.len(), 1);
        assert!(result[0].artist_name.starts_with("Unknown ("));
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

    // ── provider_image_url ──────────────────────────────────────

    #[test]
    fn provider_image_url_format() {
        assert_eq!(
            provider_image_url("tidal", "abc-123", 640),
            "/api/image/tidal/abc-123/640"
        );
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
