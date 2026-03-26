use std::{
    cmp::Ordering,
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use lofty::{
    file::{AudioFile, TaggedFileExt},
    probe::Probe,
    tag::{Accessor, ItemKey},
};
use tokio::fs;
use tracing::warn;

use crate::{
    error::{AppError, AppResult},
    util::{is_audio_extension, normalize},
};

use super::types::{DiscoveredAlbum, EmbeddedTrackMetadata, PreparedTrack, ScannedAudioFile};

pub(super) async fn discover_albums(root_path: &Path) -> AppResult<Vec<DiscoveredAlbum>> {
    let audio_files = collect_audio_files(root_path).await?;
    let mut grouped: HashMap<PathBuf, Vec<ScannedAudioFile>> = HashMap::new();

    for file_path in audio_files {
        let embedded = read_embedded_metadata(file_path.clone()).await;
        let album_dir = infer_album_directory(root_path, &file_path);
        grouped
            .entry(album_dir)
            .or_default()
            .push(ScannedAudioFile {
                absolute_path: file_path,
                embedded,
            });
    }

    let mut discovered = grouped
        .into_iter()
        .map(|(album_dir, files)| summarize_discovered_album(root_path, album_dir, files))
        .collect::<Vec<_>>();

    discovered.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(discovered)
}

pub(super) fn summarize_discovered_album(
    root_path: &Path,
    album_dir: PathBuf,
    mut files: Vec<ScannedAudioFile>,
) -> DiscoveredAlbum {
    files.sort_by(|left, right| left.absolute_path.cmp(&right.absolute_path));

    let relative_path = display_relative_path(root_path, &album_dir);
    let path_hint = path_metadata_hint(root_path, &album_dir);

    let discovered_artist = most_common_string(
        files
            .iter()
            .filter_map(|file| {
                file.embedded
                    .album_artist
                    .as_deref()
                    .or(file.embedded.track_artist.as_deref())
            })
            .collect(),
    )
    .or(path_hint.artist)
    .unwrap_or_else(|| "Unknown Artist".to_string());

    let discovered_album = most_common_string(
        files
            .iter()
            .filter_map(|file| file.embedded.album_title.as_deref())
            .collect(),
    )
    .or(path_hint.album)
    .unwrap_or_else(|| album_dir_name(&album_dir));

    let discovered_year = most_common_string(
        files
            .iter()
            .filter_map(|file| file.embedded.year.as_deref())
            .collect(),
    )
    .or(path_hint.year);

    DiscoveredAlbum {
        id: relative_path.clone(),
        relative_path,
        discovered_artist,
        discovered_album,
        discovered_year,
        files,
    }
}

pub(super) fn prepare_tracks(album: &DiscoveredAlbum) -> Vec<PreparedTrack> {
    let mut tracks = album
        .files
        .iter()
        .map(|file| PreparedTrack {
            source_path: file.absolute_path.clone(),
            title: file
                .embedded
                .track_title
                .clone()
                .unwrap_or_else(|| track_title_from_path(&file.absolute_path)),
            disc_number: file
                .embedded
                .disc_number
                .or_else(|| disc_number_from_path(&file.absolute_path)),
            track_number: file
                .embedded
                .track_number
                .or_else(|| parse_track_number_from_path(&file.absolute_path).map(|n| n as i32)),
            duration_secs: file.embedded.duration_secs,
            isrc: file.embedded.isrc.clone(),
        })
        .collect::<Vec<_>>();

    tracks.sort_by(track_sort_key);
    for (index, track) in tracks.iter_mut().enumerate() {
        if track.track_number.is_none() {
            track.track_number = i32::try_from(index + 1).ok();
        }
    }

    tracks
}

async fn collect_audio_files(root_path: &Path) -> AppResult<Vec<PathBuf>> {
    let metadata = match fs::metadata(root_path).await {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(AppError::filesystem(
                "read import root metadata",
                root_path.display().to_string(),
                err,
            ));
        }
    };

    if metadata.is_file() {
        return Ok(if is_audio_path(root_path) {
            vec![root_path.to_path_buf()]
        } else {
            Vec::new()
        });
    }

    if !metadata.is_dir() {
        return Ok(Vec::new());
    }

    let mut pending = vec![root_path.to_path_buf()];
    let mut files = Vec::new();

    while let Some(dir) = pending.pop() {
        let mut entries = fs::read_dir(&dir).await.map_err(|err| {
            AppError::filesystem("read directory", dir.display().to_string(), err)
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|err| {
            AppError::filesystem("read directory entry", dir.display().to_string(), err)
        })? {
            let path = entry.path();
            let file_type = entry.file_type().await.map_err(|err| {
                AppError::filesystem("read entry type", path.display().to_string(), err)
            })?;

            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() && is_audio_path(&path) {
                files.push(path);
            }
        }
    }

    files.sort();
    Ok(files)
}

async fn read_embedded_metadata(path: PathBuf) -> EmbeddedTrackMetadata {
    let display_path = path.display().to_string();
    match tokio::task::spawn_blocking(move || read_embedded_metadata_sync(&path)).await {
        Ok(Ok(metadata)) => metadata,
        Ok(Err(err)) => {
            warn!(path = %display_path, error = %err, "Failed to read embedded audio metadata");
            EmbeddedTrackMetadata::default()
        }
        Err(err) => {
            warn!(path = %display_path, error = %err, "Audio metadata task failed");
            EmbeddedTrackMetadata::default()
        }
    }
}

fn read_embedded_metadata_sync(path: &Path) -> AppResult<EmbeddedTrackMetadata> {
    let tagged_file = Probe::open(path)
        .map_err(|err| AppError::metadata("open embedded metadata", err.to_string()))?
        .read()
        .map_err(|err| AppError::metadata("read embedded metadata", err.to_string()))?;

    let Some(tag) = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
    else {
        return Ok(EmbeddedTrackMetadata {
            duration_secs: i32::try_from(tagged_file.properties().duration().as_secs()).ok(),
            ..EmbeddedTrackMetadata::default()
        });
    };

    Ok(EmbeddedTrackMetadata {
        album_artist: clean_string(tag.get_string(&ItemKey::AlbumArtist)),
        track_artist: tag
            .artist()
            .and_then(|value| clean_string(Some(value.as_ref()))),
        album_title: tag
            .album()
            .and_then(|value| clean_string(Some(value.as_ref()))),
        track_title: tag
            .title()
            .and_then(|value| clean_string(Some(value.as_ref()))),
        year: tag.year().map(|value| value.to_string()),
        disc_number: tag.disk().and_then(|value| i32::try_from(value).ok()),
        track_number: tag.track().and_then(|value| i32::try_from(value).ok()),
        duration_secs: i32::try_from(tagged_file.properties().duration().as_secs()).ok(),
        isrc: clean_string(tag.get_string(&ItemKey::Isrc)),
    })
}

fn track_sort_key(left: &PreparedTrack, right: &PreparedTrack) -> Ordering {
    let left_disc = left.disc_number.unwrap_or(1);
    let right_disc = right.disc_number.unwrap_or(1);
    left_disc
        .cmp(&right_disc)
        .then_with(|| {
            left.track_number
                .unwrap_or(i32::MAX)
                .cmp(&right.track_number.unwrap_or(i32::MAX))
        })
        .then_with(|| left.title.cmp(&right.title))
        .then_with(|| left.source_path.cmp(&right.source_path))
}

fn display_relative_path(root_path: &Path, album_dir: &Path) -> String {
    let relative = album_dir
        .strip_prefix(root_path)
        .unwrap_or(album_dir)
        .to_string_lossy()
        .trim_matches('/')
        .to_string();

    if relative.is_empty() {
        album_dir_name(album_dir)
    } else {
        relative
    }
}

fn is_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(is_audio_extension)
        .unwrap_or(false)
}

fn infer_album_directory(root_path: &Path, file_path: &Path) -> PathBuf {
    let Some(parent) = file_path.parent() else {
        return root_path.to_path_buf();
    };

    let Some(parent_name) = parent.file_name().and_then(OsStr::to_str) else {
        return parent.to_path_buf();
    };

    if is_disc_directory_name(parent_name)
        && let Some(grandparent) = parent.parent()
        && grandparent.starts_with(root_path)
    {
        return grandparent.to_path_buf();
    }

    parent.to_path_buf()
}

fn is_disc_directory_name(name: &str) -> bool {
    let normalized = normalize(name);
    normalized.starts_with("disc ")
        || normalized.starts_with("disk ")
        || normalized.starts_with("cd ")
        || normalized == "disc"
        || normalized == "disk"
        || normalized == "cd"
}

#[derive(Default)]
struct PathMetadataHint {
    artist: Option<String>,
    album: Option<String>,
    year: Option<String>,
}

fn path_metadata_hint(root_path: &Path, album_dir: &Path) -> PathMetadataHint {
    let album_name = album_dir_name(album_dir);
    let (album, year) = split_album_name_and_year(&album_name);
    let artist = album_dir
        .parent()
        .filter(|parent| *parent != root_path)
        .and_then(|parent| parent.file_name())
        .and_then(OsStr::to_str)
        .and_then(|value| clean_string(Some(value)));

    PathMetadataHint {
        artist,
        album,
        year,
    }
}

fn album_dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(OsStr::to_str)
        .and_then(|value| clean_string(Some(value)))
        .unwrap_or_else(|| "Unknown Album".to_string())
}

fn split_album_name_and_year(input: &str) -> (Option<String>, Option<String>) {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return (None, None);
    }

    if let Some(year) = trailing_year(trimmed) {
        let suffix = format!("({year})");
        let bracketed = format!("[{year}]");
        let title = trimmed
            .strip_suffix(&suffix)
            .or_else(|| trimmed.strip_suffix(&bracketed))
            .unwrap_or(trimmed)
            .trim_end_matches([' ', '-', '_'])
            .trim();
        return (clean_string(Some(title)), Some(year));
    }

    if let Some((year, title)) = leading_year(trimmed) {
        return (clean_string(Some(title)), Some(year));
    }

    (clean_string(Some(trimmed)), extract_year_candidate(trimmed))
}

fn trailing_year(input: &str) -> Option<String> {
    let trimmed = input.trim_end();
    if trimmed.len() < 6 {
        return None;
    }

    let end = trimmed.len();
    let bytes = trimmed.as_bytes();
    let suffix = &bytes[end.saturating_sub(6)..end];
    let starts = suffix.first().copied();
    let ends = suffix.last().copied();
    if !matches!(starts, Some(b'(') | Some(b'[')) || !matches!(ends, Some(b')') | Some(b']')) {
        return None;
    }

    let digits = &trimmed[end - 5..end - 1];
    normalize_year(digits)
}

fn leading_year(input: &str) -> Option<(String, &str)> {
    let trimmed = input.trim_start();
    if trimmed.len() < 4 {
        return None;
    }

    let year = normalize_year(&trimmed[..4])?;
    let rest = trimmed[4..].trim_start_matches([' ', '-', '_', '.', ')', ']']);
    if rest.is_empty() {
        None
    } else {
        Some((year, rest))
    }
}

fn extract_year_candidate(input: &str) -> Option<String> {
    let chars = input.chars().collect::<Vec<_>>();
    if chars.len() < 4 {
        return None;
    }

    for window in chars.windows(4) {
        let candidate = window.iter().collect::<String>();
        if let Some(year) = normalize_year(&candidate) {
            return Some(year);
        }
    }

    None
}

fn normalize_year(value: &str) -> Option<String> {
    if value.len() == 4
        && value.chars().all(|char| char.is_ascii_digit())
        && let Ok(year) = value.parse::<i32>()
        && (1900..=2100).contains(&year)
    {
        return Some(year.to_string());
    }

    None
}

fn track_title_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .map(str::trim)
        .unwrap_or("Unknown Track");

    let stripped = stem
        .trim_start_matches(|char: char| char.is_ascii_digit())
        .trim_start_matches([' ', '-', '.', '_']);

    if stripped.is_empty() {
        stem.to_string()
    } else {
        stripped.to_string()
    }
}

fn parse_track_number_from_path(path: &Path) -> Option<u32> {
    let stem = path.file_stem()?.to_str()?.trim();
    let digits = stem
        .chars()
        .take_while(|char| char.is_ascii_digit())
        .collect::<String>();

    if digits.is_empty() {
        None
    } else {
        digits.parse::<u32>().ok()
    }
}

fn disc_number_from_path(path: &Path) -> Option<i32> {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(OsStr::to_str)
        .and_then(|name| {
            let normalized = normalize(name);
            for prefix in ["disc ", "disk ", "cd "] {
                if let Some(value) = normalized.strip_prefix(prefix) {
                    return value.parse::<i32>().ok();
                }
            }

            if let Some(value) = normalized.strip_prefix("disc") {
                return value.trim().parse::<i32>().ok();
            }
            if let Some(value) = normalized.strip_prefix("disk") {
                return value.trim().parse::<i32>().ok();
            }
            if let Some(value) = normalized.strip_prefix("cd") {
                return value.trim().parse::<i32>().ok();
            }

            None
        })
}

fn most_common_string(values: Vec<&str>) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for value in values {
        if let Some(value) = clean_string(Some(value)) {
            *counts.entry(value).or_default() += 1;
        }
    }

    counts
        .into_iter()
        .max_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| left.0.len().cmp(&right.0.len()))
        })
        .map(|(value, _)| value)
}

fn clean_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
