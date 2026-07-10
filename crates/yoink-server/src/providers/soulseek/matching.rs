//! Candidate scoring and album-bundle selection for SoulSeek search results.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

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
    track_number: Option<u32>,
    length: Option<u32>,
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
            if detect_audio_extension(file.extension.as_deref(), &file.filename).is_none() {
                continue;
            }
            if !is_plausible_match(&file.filename, file.length, ctx) {
                continue;
            }

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
    if !artist.is_empty() && filename.contains(artist) {
        score += 45;
    }
    if !album.is_empty() && filename.contains(album) {
        score += 20;
    }
    if !title.is_empty() && filename.contains(title) {
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

/// Reject files that only happen to contain the requested title as part of a
/// different title. Loose names require both close duration and artist/album
/// context; exact-compatible names can stand on their own.
fn is_plausible_match(
    filename: &str,
    candidate_duration: Option<u32>,
    ctx: &DownloadTrackContext,
) -> bool {
    let title_tokens = tokens(&ctx.track_title);
    if title_tokens.is_empty() {
        return false;
    }

    let normalized_path = filename.replace('\\', "/");
    let stem = Path::new(&normalized_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename);
    let mut remaining = tokens(stem);

    for expected in &title_tokens {
        let Some(position) = remaining.iter().position(|token| token == expected) else {
            return false;
        };
        remaining.remove(position);
    }

    let artist_tokens = tokens(&ctx.artist_name);
    let album_tokens = tokens(&ctx.album_title);
    let strong_title = remaining.iter().all(|token| {
        token.chars().all(|c| c.is_ascii_digit())
            || artist_tokens.contains(token)
            || album_tokens.contains(token)
            || TITLE_NOISE_TOKENS.contains(&token.as_str())
    });

    let duration_diff = candidate_duration
        .zip(ctx.duration_secs)
        .map(|(candidate, expected)| candidate.abs_diff(expected));
    if let (Some(diff), Some(expected)) = (duration_diff, ctx.duration_secs) {
        let tolerance = 8.max(expected / 20);
        if diff > tolerance {
            return false;
        }
    }

    if strong_title {
        return true;
    }

    let normalized_filename = normalize(filename);
    let has_context = (!artist_tokens.is_empty()
        && !is_unknown_artist(&ctx.artist_name)
        && normalized_filename.contains(&normalize(&ctx.artist_name)))
        || (!album_tokens.is_empty() && normalized_filename.contains(&normalize(&ctx.album_title)));

    has_context && duration_diff.is_some_and(|diff| diff <= 3)
}

fn tokens(value: &str) -> Vec<String> {
    normalize(value)
        .split_whitespace()
        .map(ToString::to_string)
        .collect()
}

fn is_unknown_artist(value: &str) -> bool {
    matches!(normalize(value).as_str(), "unknown" | "unknown artist")
}

const TITLE_NOISE_TOKENS: &[&str] = &[
    "album",
    "cd",
    "clean",
    "edit",
    "explicit",
    "flac",
    "lossless",
    "master",
    "mix",
    "original",
    "remaster",
    "remastered",
    "stereo",
    "track",
    "version",
];

fn file_extension(file: &SearchFile) -> String {
    detect_audio_extension(file.extension.as_deref(), &file.filename).unwrap_or_default()
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

    bundles
        .into_iter()
        .filter(|(_, files, _)| count_unique_tracks(files) >= expected_tracks)
        .filter_map(|(key, files, _)| {
            let chosen = choose_track_from_bundle(&files, ctx, quality)?;
            let bundle_score = score_bundle(&key.1, &artist, &album, &files, expected_tracks);
            Some(Candidate {
                username: chosen.username.clone(),
                filename: chosen.filename.clone(),
                size: chosen.size,
                score: 10_000 + bundle_score,
            })
        })
        .max_by_key(|candidate| candidate.score)
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
                    track_number: parse_track_number(&file.filename),
                    length: file.length,
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
    let require_track_number =
        ctx.track_number.is_some() && files.iter().any(|file| file.track_number.is_some());

    files
        .iter()
        .filter(|file| !require_track_number || file.track_number == ctx.track_number)
        .filter(|file| is_plausible_match(&file.filename, file.length, ctx))
        .max_by_key(|file| {
            let track_number_matches =
                ctx.track_number.is_some() && ctx.track_number == file.track_number;
            (
                track_number_matches,
                bundle_extension_quality_score(&file.extension, quality),
            )
        })
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
