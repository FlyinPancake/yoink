//! Candidate scoring and album-bundle selection for SoulSeek search results.

use std::collections::{HashMap, HashSet};

use super::{
    models::{SearchFile, SearchResponse},
    util::{detect_audio_extension, normalize, normalized_parent_dir, parse_track_number},
};
use crate::{db::quality::Quality, providers::DownloadTrackContext};

// ── Types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub username: String,
    pub filename: String,
    pub size: i64,
    pub score: i32,
}

#[derive(Debug, Clone)]
struct AlbumBundleFile {
    username: String,
    filename: String,
    size: i64,
    extension: String,
    normalized_filename: String,
    track_number: Option<u32>,
}

// ── Single-track scoring ────────────────────────────────────────────

pub(crate) fn pick_best_candidate(
    responses: &[SearchResponse],
    ctx: &DownloadTrackContext,
    quality: &Quality,
) -> Option<Candidate> {
    let artist = normalize(&ctx.artist_name);
    let album = normalize(&ctx.album_title);
    let title = normalize(&ctx.track_title);

    let mut best: Option<Candidate> = None;

    for resp in responses {
        for file in &resp.files {
            let score = score_file(file, &artist, &album, &title, ctx, quality);

            if best.as_ref().is_none_or(|b| score > b.score) {
                best = Some(Candidate {
                    username: resp.username.clone(),
                    filename: file.filename.clone(),
                    size: file.size,
                    score,
                });
            }
        }
    }

    best
}

fn score_file(
    file: &SearchFile,
    artist: &str,
    album: &str,
    title: &str,
    ctx: &DownloadTrackContext,
    quality: &Quality,
) -> i32 {
    let filename = normalize(&file.filename);
    let mut score = 0i32;

    // Metadata matches
    if filename.contains(artist) {
        score += 45;
    }
    if filename.contains(album) {
        score += 20;
    }
    if filename.contains(title) {
        score += 60;
    }

    // Duration proximity
    if let Some(len) = file.length
        && let Some(target_secs) = ctx.duration_secs
    {
        let diff = (len as i32 - target_secs as i32).abs();
        score += match diff {
            0..=2 => 20,
            3..=5 => 10,
            6..=15 => 4,
            _ => -10,
        };
    }

    // Format preference
    let ext = file_extension(file);
    score += extension_quality_score(&ext, quality);

    // Bitrate bonus
    if let Some(bitrate) = file.bit_rate {
        if bitrate >= 900 {
            score += 10;
        } else if bitrate >= 320 {
            score += 4;
        }
    }

    score
}

fn file_extension(file: &SearchFile) -> String {
    file.extension
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn extension_quality_score(ext: &str, quality: &Quality) -> i32 {
    match quality {
        Quality::HiRes | Quality::Lossless => match ext {
            "flac" => 30,
            "m4a" | "alac" => 6,
            "wav" => 0,
            _ => -12,
        },
        Quality::High | Quality::Low => match ext {
            "mp3" | "ogg" | "aac" => 6,
            _ => -12,
        },
    }
}

// ── Album-bundle selection ──────────────────────────────────────────

/// Try to find a complete album folder and pick the requested track from it.
/// Returns `None` if no folder has at least `expected_tracks` audio files.
pub(crate) fn pick_from_album_bundle(
    responses: &[SearchResponse],
    ctx: &DownloadTrackContext,
    quality: &Quality,
) -> Option<Candidate> {
    let expected_tracks = ctx.album_track_count.filter(|&n| n > 0)?;

    let bundles = group_files_into_bundles(responses);

    let artist = normalize(&ctx.artist_name);
    let album = normalize(&ctx.album_title);

    let (_, best_files, _) = bundles
        .into_iter()
        .filter(|(_, files, _)| count_unique_tracks(files) >= expected_tracks)
        .map(|(key, files, _)| {
            let score = score_bundle(&key.1, &artist, &album, &files, expected_tracks);
            (key, files, score)
        })
        .max_by_key(|(_, _, score)| *score)?;

    let chosen = choose_track_from_bundle(&best_files, ctx, quality)?;

    Some(Candidate {
        username: chosen.username.clone(),
        filename: chosen.filename.clone(),
        size: chosen.size,
        score: 10_000, // Album-bundle matches are always preferred.
    })
}

type BundleKey = (String, String); // (username, parent_dir)

fn group_files_into_bundles(
    responses: &[SearchResponse],
) -> Vec<(BundleKey, Vec<AlbumBundleFile>, i32)> {
    let mut map: HashMap<BundleKey, Vec<AlbumBundleFile>> = HashMap::new();

    for resp in responses {
        for file in &resp.files {
            let Some(extension) = detect_audio_extension(file.extension.as_deref(), &file.filename)
            else {
                continue;
            };
            let parent = normalized_parent_dir(&file.filename);
            if parent.is_empty() {
                continue;
            }

            map.entry((resp.username.clone(), parent))
                .or_default()
                .push(AlbumBundleFile {
                    username: resp.username.clone(),
                    filename: file.filename.clone(),
                    size: file.size,
                    extension,
                    normalized_filename: normalize(&file.filename),
                    track_number: parse_track_number(&file.filename),
                });
        }
    }

    map.into_iter().map(|(k, v)| (k, v, 0)).collect()
}

fn count_unique_tracks(files: &[AlbumBundleFile]) -> usize {
    let numbers: HashSet<u32> = files.iter().filter_map(|f| f.track_number).collect();
    if numbers.is_empty() {
        files.len()
    } else {
        numbers.len()
    }
}

fn score_bundle(
    parent_dir: &str,
    artist: &str,
    album: &str,
    files: &[AlbumBundleFile],
    expected_tracks: usize,
) -> i32 {
    let parent_norm = normalize(parent_dir);
    let mut score = 0i32;

    if !artist.is_empty() && parent_norm.contains(artist) {
        score += 35;
    }
    if !album.is_empty() && parent_norm.contains(album) {
        score += 50;
    }

    let flac_count = files.iter().filter(|f| f.extension == "flac").count() as i32;
    score += flac_count * 2;
    score -= (count_unique_tracks(files) as i32 - expected_tracks as i32).abs();

    score
}

fn choose_track_from_bundle<'a>(
    files: &'a [AlbumBundleFile],
    ctx: &DownloadTrackContext,
    quality: &Quality,
) -> Option<&'a AlbumBundleFile> {
    let by_quality = |f: &&AlbumBundleFile| bundle_extension_quality_score(&f.extension, quality);

    // Prefer matching by track number (most reliable for album bundles).
    if let Some(track_number) = ctx.track_number {
        let best = files
            .iter()
            .filter(|f| f.track_number == Some(track_number))
            .max_by_key(by_quality);
        if best.is_some() {
            return best;
        }
    }

    // Fall back to title matching.
    let title = normalize(&ctx.track_title);
    if !title.is_empty() {
        let best = files
            .iter()
            .filter(|f| f.normalized_filename.contains(&title))
            .max_by_key(by_quality);
        if best.is_some() {
            // TODO: add fuzzy title matching within selected album bundle.
            return best;
        }
    }

    // Last resort: pick highest quality file in the bundle.
    files.iter().max_by_key(by_quality)
}

// ── Quality scoring for bundle file selection ───────────────────────

/// Score an extension for bundle-internal file selection (broader range than
/// the single-track scorer since we already trust the bundle).
fn bundle_extension_quality_score(ext: &str, quality: &Quality) -> i32 {
    match quality {
        Quality::HiRes | Quality::Lossless => match ext {
            "flac" => 100,
            "m4a" | "alac" => 60,
            "wav" => 40,
            "aac" | "ogg" | "mp3" => 10,
            _ => 0,
        },
        Quality::High | Quality::Low => match ext {
            "mp3" | "ogg" | "aac" => 60,
            "flac" => 30,
            "m4a" | "alac" => 20,
            "wav" => 10,
            _ => 0,
        },
    }
}
