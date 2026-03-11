//! Cross-domain utilities shared within the yoink-server crate.

// ── Text normalisation ──────────────────────────────────────────────

/// Normalize text for fuzzy comparison: lowercase (Unicode-aware), strip
/// non-ASCII-alphanumeric characters, and collapse whitespace.
pub(crate) fn normalize(input: &str) -> String {
    input
        .chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ── Provider ranking ────────────────────────────────────────────────

/// Higher value = preferred as display-metadata source.
pub(crate) fn provider_priority(provider_id: &str) -> u8 {
    match provider_id {
        "tidal" => 10,
        "deezer" => 9,
        "musicbrainz" => 1,
        _ => 5,
    }
}

// ── Audio file extensions ───────────────────────────────────────────

/// Canonical list of recognised audio file extensions (lowercase).
pub(crate) const AUDIO_EXTENSIONS: &[&str] =
    &["flac", "m4a", "mp4", "alac", "mp3", "ogg", "wav", "aac"];

/// Check whether `ext` (case-insensitive) is a known audio extension.
pub(crate) fn is_audio_extension(ext: &str) -> bool {
    AUDIO_EXTENSIONS
        .iter()
        .any(|&a| a.eq_ignore_ascii_case(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalize ───────────────────────────────────────────────

    #[test]
    fn normalize_lowercases() {
        assert_eq!(normalize("HELLO WORLD"), "hello world");
    }

    #[test]
    fn normalize_strips_non_alphanumeric() {
        assert_eq!(normalize("hello-world"), "hello world");
        assert_eq!(normalize("hello.world!"), "hello world");
    }

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize("hello   world"), "hello world");
        assert_eq!(normalize("  spaced  out  "), "spaced out");
    }

    #[test]
    fn normalize_complex() {
        assert_eq!(normalize("The Black Keys (Live)"), "the black keys live");
    }

    // ── provider_priority ───────────────────────────────────────

    #[test]
    fn provider_priority_known() {
        assert_eq!(provider_priority("tidal"), 10);
        assert_eq!(provider_priority("deezer"), 9);
        assert_eq!(provider_priority("musicbrainz"), 1);
    }

    #[test]
    fn provider_priority_unknown() {
        assert_eq!(provider_priority("spotify"), 5);
        assert_eq!(provider_priority("bandcamp"), 5);
    }

    // ── is_audio_extension ──────────────────────────────────────

    #[test]
    fn recognises_known_extensions() {
        for ext in AUDIO_EXTENSIONS {
            assert!(is_audio_extension(ext), "should recognise {ext}");
        }
    }

    #[test]
    fn case_insensitive() {
        assert!(is_audio_extension("FLAC"));
        assert!(is_audio_extension("Mp3"));
    }

    #[test]
    fn rejects_unknown_extension() {
        assert!(!is_audio_extension("txt"));
        assert!(!is_audio_extension("jpg"));
    }
}
