//! String normalization, path sanitization, and encoding helpers.

use std::path::{Component, Path, PathBuf};

pub(crate) use crate::util::normalize;

/// Percent-encode a string for use in URL path segments (RFC 3986 unreserved set).
pub(crate) fn percent_encode_path(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

/// Strip path traversal and root components, normalizing backslashes to `/`.
pub(crate) fn sanitize_relative_path(input: &str) -> PathBuf {
    let relative = input.replace('\\', "/").trim_start_matches('/').to_string();
    let path = Path::new(&relative);
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {}
        }
    }
    out
}

/// Deduplicate a list of query strings while preserving insertion order.
/// Drops empty entries.
pub(crate) fn dedup_queries(queries: Vec<String>) -> Vec<String> {
    let mut seen = Vec::new();
    for q in queries {
        let q = q.trim().to_string();
        if !q.is_empty() && !seen.contains(&q) {
            seen.push(q);
        }
    }
    seen
}

/// Extract the parent directory from a SoulSeek-style path (backslash-separated),
/// normalized to forward slashes.
pub(crate) fn normalized_parent_dir(filename: &str) -> String {
    let normalized = filename.replace('\\', "/");
    Path::new(&normalized)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Try to extract a leading track number from the file stem.
pub(crate) fn parse_track_number(filename: &str) -> Option<u32> {
    let normalized = filename.replace('\\', "/");
    let stem = Path::new(&normalized).file_stem()?.to_str()?;
    let digits: String = stem
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

pub(crate) use crate::util::is_audio_extension;

/// Detect the audio file extension from slskd metadata or the filename itself.
pub(crate) fn detect_audio_extension(
    extension_field: Option<&str>,
    filename: &str,
) -> Option<String> {
    let ext = extension_field
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_ascii_lowercase())
        .or_else(|| {
            Path::new(filename)
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
        })?;

    if is_audio_extension(&ext) {
        Some(ext)
    } else {
        None
    }
}
