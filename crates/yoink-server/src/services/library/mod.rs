mod browse;
mod import;
mod merge;
mod reconcile;
mod sync;

pub(crate) use browse::browse_path;
pub(crate) use import::{
    confirm_external_import, confirm_import_library, preview_external_import,
    preview_import_library, scan_and_import_library,
};
pub(crate) use merge::merge_albums;
pub(crate) use reconcile::reconcile_library_files;
pub(crate) use sync::{sync_album_tracks, sync_artist};

use crate::util::{is_audio_extension, normalize as normalize_text};

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
    use super::*;

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
