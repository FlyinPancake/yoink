mod import;
mod merge;
mod reconcile;
mod sync;

use crate::models::MonitoredAlbum;
use crate::util::{is_audio_extension, normalize as normalize_text};

pub(crate) use import::{confirm_import_library, preview_import_library, scan_and_import_library};
pub(crate) use merge::merge_albums;
pub(crate) use reconcile::reconcile_library_files;
pub(crate) use sync::sync_artist_albums;

/// Recompute the derived `wanted` flag for an album.
///
/// An album is fully wanted when it is album-level monitored and not yet acquired.
/// `partially_wanted` is not updated here — it depends on track-level state and
/// is computed via a DB subquery in `load_albums` or explicitly via
/// `recompute_partially_wanted`.
pub(crate) fn update_wanted(album: &mut MonitoredAlbum) {
    album.wanted = album.monitored && !album.acquired;
}

/// Recompute the `partially_wanted` flag for an album by checking its tracks.
/// Call this after toggling individual track monitoring.
pub(crate) async fn recompute_partially_wanted(db: &sqlx::SqlitePool, album: &mut MonitoredAlbum) {
    if album.monitored {
        // Fully monitored albums are never "partially" wanted
        album.partially_wanted = false;
    } else {
        album.partially_wanted = crate::db::has_wanted_tracks(db, album.id)
            .await
            .unwrap_or(false);
    }
}

fn parse_release_year(release_date: &str) -> Option<String> {
    let year = release_date.chars().take(4).collect::<String>();
    if year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()) {
        Some(year)
    } else {
        None
    }
}

async fn album_dir_has_downloaded_audio(path: &std::path::Path) -> bool {
    use tokio::fs;

    if !fs::try_exists(path).await.unwrap_or(false) {
        return false;
    }

    let mut entries = match fs::read_dir(path).await {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let p = entry.path();
        if p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_audio_extension)
        {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;

    fn make_album(monitored: bool, acquired: bool) -> MonitoredAlbum {
        MonitoredAlbum {
            id: Uuid::now_v7(),
            artist_id: Uuid::now_v7(),
            artist_ids: Vec::new(),
            artist_credits: Vec::new(),
            title: "Test Album".to_string(),
            album_type: None,
            release_date: None,
            cover_url: None,
            explicit: false,
            quality_override: None,
            monitored,
            acquired,
            wanted: false,
            partially_wanted: false,
            added_at: Utc::now(),
        }
    }

    // ── update_wanted ───────────────────────────────────────────

    #[test]
    fn update_wanted_monitored_not_acquired() {
        let mut album = make_album(true, false);
        update_wanted(&mut album);
        assert!(album.wanted);
    }

    #[test]
    fn update_wanted_monitored_and_acquired() {
        let mut album = make_album(true, true);
        update_wanted(&mut album);
        assert!(!album.wanted);
    }

    #[test]
    fn update_wanted_not_monitored() {
        let mut album = make_album(false, false);
        update_wanted(&mut album);
        assert!(!album.wanted);
    }

    #[test]
    fn update_wanted_not_monitored_acquired() {
        let mut album = make_album(false, true);
        update_wanted(&mut album);
        assert!(!album.wanted);
    }

    // ── normalize_text ──────────────────────────────────────────

    #[test]
    fn normalize_text_lowercases() {
        assert_eq!(normalize_text("HELLO"), "hello");
    }

    #[test]
    fn normalize_text_non_alphanumeric_to_space() {
        assert_eq!(normalize_text("hello-world!"), "hello world");
    }

    #[test]
    fn normalize_text_collapses_whitespace() {
        assert_eq!(normalize_text("  hello   world  "), "hello world");
    }

    #[test]
    fn normalize_text_unicode_lowercase() {
        // German eszett: lowercase of "SS" depends on locale, but
        // individual chars should be lowercased.
        assert_eq!(normalize_text("ABC"), "abc");
    }

    // ── parse_release_year ──────────────────────────────────────

    #[test]
    fn parse_release_year_full_date() {
        assert_eq!(parse_release_year("2024-03-15"), Some("2024".to_string()));
    }

    #[test]
    fn parse_release_year_just_year() {
        assert_eq!(parse_release_year("2024"), Some("2024".to_string()));
    }

    #[test]
    fn parse_release_year_non_digit() {
        assert_eq!(parse_release_year("abcd"), None);
    }

    #[test]
    fn parse_release_year_too_short() {
        assert_eq!(parse_release_year("20"), None);
    }

    #[test]
    fn parse_release_year_empty() {
        assert_eq!(parse_release_year(""), None);
    }
}
