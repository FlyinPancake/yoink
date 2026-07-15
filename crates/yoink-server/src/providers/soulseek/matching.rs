//! Candidate scoring and album-bundle selection for SoulSeek search results.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use super::{
    models::{SearchFile, SearchResponse},
    util::{detect_audio_extension, normalize, normalized_parent_dir, parse_track_number},
};
use crate::{
    db::quality::Quality,
    providers::{
        DownloadTrackContext, ManualAlbumCandidate, ManualAlbumFile, ManualSearchCandidate,
    },
};

// ── Types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub username: String,
    pub filename: String,
    pub size: i64,
    pub score: i32,
    /// Duration advertised in the search result, used to verify the download.
    pub reported_length: Option<u32>,
}

#[derive(Debug, Clone)]
struct AlbumBundleFile {
    username: String,
    filename: String,
    size: i64,
    extension: String,
    track_number: Option<u32>,
    length: Option<u32>,
    bit_rate: Option<u32>,
}

// ── Single-track scoring ────────────────────────────────────────────

/// Reject single-track candidates whose score falls below this floor; a
/// negative score means nothing beyond bare title plausibility matched.
const MIN_SINGLE_TRACK_SCORE: i32 = 0;

/// Score all plausible files across responses and return them best-first, so
/// callers can fall back to the next candidate when a download fails.
pub(crate) fn rank_candidates(
    responses: &[SearchResponse],
    ctx: &DownloadTrackContext,
    quality: &Quality,
) -> Vec<Candidate> {
    let artist = normalize(&ctx.artist_name);
    let album = normalize(&ctx.album_title);
    let title = normalize(&ctx.track_title);

    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut candidates = Vec::new();
    let (mut non_audio, mut implausible, mut low_score) = (0usize, 0usize, 0usize);
    let mut rejected_examples: Vec<String> = Vec::new();

    for resp in responses {
        for file in &resp.files {
            if detect_audio_extension(file.extension.as_deref(), &file.filename).is_none() {
                non_audio += 1;
                continue;
            }
            if !is_plausible_match(&file.filename, file.length, ctx) {
                implausible += 1;
                if rejected_examples.len() < 3 {
                    rejected_examples.push(format!("{} len={:?}", file.filename, file.length));
                }
                continue;
            }

            let score = score_file(file, &artist, &album, &title, ctx, quality) + peer_score(resp);
            if score < MIN_SINGLE_TRACK_SCORE {
                low_score += 1;
                continue;
            }
            if !seen.insert((resp.username.clone(), file.filename.clone())) {
                continue;
            }

            candidates.push(Candidate {
                username: resp.username.clone(),
                filename: file.filename.clone(),
                size: file.size,
                score,
                reported_length: file.length,
            });
        }
    }

    if candidates.is_empty() {
        tracing::debug!(
            track = ctx.track_title,
            non_audio,
            implausible,
            low_score,
            examples = ?rejected_examples,
            "SoulSeek ranking rejected every file"
        );
    }

    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));
    candidates
}

/// Score every audio file across responses without filtering, for manual
/// (interactive) search. Plausible files sort first, then by score.
pub(crate) fn rank_all_files(
    responses: &[SearchResponse],
    ctx: &DownloadTrackContext,
    quality: &Quality,
) -> Vec<ManualSearchCandidate> {
    let artist = normalize(&ctx.artist_name);
    let album = normalize(&ctx.album_title);
    let title = normalize(&ctx.track_title);

    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut candidates = Vec::new();

    for resp in responses {
        for file in &resp.files {
            let extension = detect_audio_extension(file.extension.as_deref(), &file.filename);
            if extension.is_none() {
                continue;
            }
            if !seen.insert((resp.username.clone(), file.filename.clone())) {
                continue;
            }

            candidates.push(ManualSearchCandidate {
                username: resp.username.clone(),
                filename: file.filename.clone(),
                size: file.size,
                length_secs: file.length,
                bit_rate: file.bit_rate,
                extension,
                score: score_file(file, &artist, &album, &title, ctx, quality) + peer_score(resp),
                plausible: is_plausible_match(&file.filename, file.length, ctx),
                has_free_upload_slot: resp.has_free_upload_slot,
                queue_length: resp.queue_length,
            });
        }
    }

    candidates.sort_by_key(|candidate| {
        (
            std::cmp::Reverse(candidate.plausible),
            std::cmp::Reverse(candidate.score),
        )
    });
    candidates
}

/// Small bonus for peers likely to actually serve the file; kept minor so
/// metadata quality still dominates ranking.
fn peer_score(resp: &SearchResponse) -> i32 {
    let mut score = 0i32;
    if resp.has_free_upload_slot {
        score += 6;
    } else if resp.queue_length > 20 {
        score -= 4;
    }
    if resp.upload_speed >= 1_000_000 {
        score += 2;
    }
    score
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
    let filename_tokens: HashSet<&str> = filename.split(' ').collect();
    let mut score = 0i32;

    // Metadata matches: full credit for a contiguous match, partial credit
    // when all tokens are present but interleaved with other text.
    score += containment_score(&filename, &filename_tokens, artist, 45, 35);
    score += containment_score(&filename, &filename_tokens, album, 20, 14);
    score += containment_score(&filename, &filename_tokens, title, 60, 40);

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

fn containment_score(
    filename: &str,
    filename_tokens: &HashSet<&str>,
    needle: &str,
    contiguous: i32,
    scattered: i32,
) -> i32 {
    if needle.is_empty() {
        return 0;
    }
    if filename.contains(needle) {
        return contiguous;
    }
    if needle
        .split(' ')
        .all(|token| filename_tokens.contains(token))
    {
        return scattered;
    }
    0
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
    let stem_tokens = tokens(stem);

    let mut remaining = stem_tokens.clone();
    let mut missing = 0usize;
    for expected in &title_tokens {
        match remaining.iter().position(|token| token == expected) {
            Some(position) => {
                remaining.remove(position);
            }
            None => missing += 1,
        }
    }

    // Space-insensitive fallback for joined/split words ("Skyhigh" vs
    // "Sky High", "4 Ever" vs "4ever"): match the title against the stem with
    // all spaces removed and keep what surrounds it as the leftover.
    let compact_leftover = (missing > 0)
        .then(|| {
            let stem_compact = stem_tokens.concat();
            let title_compact = title_tokens.concat();
            stem_compact.find(&title_compact).map(|start| {
                let mut leftover = stem_compact;
                leftover.replace_range(start..start + title_compact.len(), "");
                leftover
            })
        })
        .flatten();

    // Filenames often drop or vary one word of a longer title ("Pt" vs
    // "Part"); forgive a single absent token, but such matches must clear the
    // stricter context + duration gate below instead of standing on their own.
    if compact_leftover.is_none() && (missing > 1 || (missing == 1 && title_tokens.len() < 3)) {
        return false;
    }

    let artist_tokens = tokens(&ctx.artist_name);
    let album_tokens = tokens(&ctx.album_title);

    // Leftover markers of a different recording (instrumental, live, cover,
    // ...) disqualify the file unless the request itself asks for that
    // version. After a compact match the per-token leftover is meaningless,
    // so check every stem token that is not part of the title instead.
    let version_check_tokens: &[String] = if compact_leftover.is_some() {
        &stem_tokens
    } else {
        &remaining
    };
    if version_check_tokens.iter().any(|token| {
        WRONG_VERSION_TOKENS.contains(&token.as_str())
            && !title_tokens.contains(token)
            && !album_tokens.contains(token)
    }) {
        return false;
    }

    let strong_title = if missing == 0 {
        remaining.iter().all(|token| {
            token.chars().all(|c| c.is_ascii_digit())
                || artist_tokens.contains(token)
                || album_tokens.contains(token)
                || TITLE_NOISE_TOKENS.contains(&token.as_str())
        })
    } else if let Some(leftover) = &compact_leftover {
        compact_leftover_is_noise(leftover, &artist_tokens, &album_tokens)
    } else {
        false
    };

    let duration_diff = candidate_duration
        .zip(ctx.duration_secs)
        .map(|(candidate, expected)| candidate.abs_diff(expected));
    if let (Some(diff), Some(expected)) = (duration_diff, ctx.duration_secs) {
        // Provider durations can be well off for the same recording (megamix
        // segment boundaries, pressing differences), so a strong title match
        // gets a generous bound and relies on the score penalty instead. A
        // weak match keeps the tight bound: there duration is the main
        // defense against a different song.
        let tolerance = if strong_title {
            45.max(expected / 3)
        } else {
            8.max(expected / 20)
        };
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

/// After removing the title from the space-stripped stem, decide whether the
/// leftover is only numbering/artist/album/noise — i.e. nothing hinting at a
/// different song.
fn compact_leftover_is_noise(
    leftover: &str,
    artist_tokens: &[String],
    album_tokens: &[String],
) -> bool {
    let mut leftover = leftover.to_string();
    for chunk in [artist_tokens.concat(), album_tokens.concat()] {
        if !chunk.is_empty() {
            leftover = leftover.replace(&chunk, "");
        }
    }
    for token in TITLE_NOISE_TOKENS {
        leftover = leftover.replace(token, "");
    }
    leftover.chars().all(|c| c.is_ascii_digit())
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
    "feat",
    "featuring",
    "flac",
    "ft",
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

/// Tokens that mark a different recording of the requested song. Their
/// presence in leftover filename tokens rejects the candidate outright unless
/// the requested title or album contains the token itself.
const WRONG_VERSION_TOKENS: &[&str] = &[
    "8d",
    "acapella",
    "acoustic",
    "bootleg",
    "cover",
    "demo",
    "instrumental",
    "karaoke",
    "live",
    "mashup",
    "medley",
    "megamix",
    "nightcore",
    "remix",
    "reverb",
    "slowed",
    "sped",
    "tribute",
    "unplugged",
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

/// Find complete album folders and pick the requested track from each,
/// returned best-first. Folders whose path mentions neither the artist nor
/// the album are skipped: a track-count coincidence alone must not outrank
/// single-file matches.
pub(crate) fn rank_album_bundles(
    responses: &[SearchResponse],
    ctx: &DownloadTrackContext,
    quality: &Quality,
) -> Vec<Candidate> {
    let Some(expected_tracks) = ctx.album_track_count.filter(|&n| n > 0) else {
        return Vec::new();
    };

    let peer_scores: HashMap<&str, i32> = responses
        .iter()
        .map(|resp| (resp.username.as_str(), peer_score(resp)))
        .collect();
    let bundles = group_files_into_bundles(responses);

    let artist = normalize(&ctx.artist_name);
    let album = normalize(&ctx.album_title);

    let mut candidates: Vec<Candidate> = bundles
        .into_iter()
        .filter(|(_, files)| count_unique_tracks(files) >= expected_tracks)
        .filter_map(|(key, files)| {
            let parent_norm = normalize(&key.1);
            let has_context = (!artist.is_empty() && parent_norm.contains(&artist))
                || (!album.is_empty() && parent_norm.contains(&album));
            if !has_context {
                return None;
            }

            let chosen = choose_track_from_bundle(&files, ctx, quality)?;
            let bundle_score = score_bundle(&key.1, &artist, &album, &files, expected_tracks)
                + peer_scores.get(key.0.as_str()).copied().unwrap_or(0);
            Some(Candidate {
                username: chosen.username.clone(),
                filename: chosen.filename.clone(),
                size: chosen.size,
                score: 10_000 + bundle_score,
                reported_length: chosen.length,
            })
        })
        .collect();

    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));
    candidates
}

type BundleKey = (String, String); // (username, parent_dir)

fn group_files_into_bundles(
    responses: &[SearchResponse],
) -> Vec<(BundleKey, Vec<AlbumBundleFile>)> {
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
                    bit_rate: file.bit_rate,
                });
        }
    }

    map.into_iter().collect()
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

/// Rank every album folder peers offered, best-matched first, for manual
/// (interactive) album search. Unlike [`rank_album_bundles`] nothing is
/// filtered out — the user decides.
pub(crate) fn rank_album_folders(
    responses: &[SearchResponse],
    tracks: &[DownloadTrackContext],
    quality: &Quality,
) -> Vec<ManualAlbumCandidate> {
    let Some(first) = tracks.first() else {
        return Vec::new();
    };
    let artist = normalize(&first.artist_name);
    let album = normalize(&first.album_title);
    let expected_tracks = tracks.len();

    let peers: HashMap<&str, &SearchResponse> = responses
        .iter()
        .map(|resp| (resp.username.as_str(), resp))
        .collect();

    let mut candidates: Vec<ManualAlbumCandidate> = group_files_into_bundles(responses)
        .into_iter()
        .map(|((username, folder), files)| {
            let matched_tracks = tracks
                .iter()
                .filter(|ctx| pick_manual_bundle_file(&files, ctx, quality).is_some())
                .count() as u32;
            let peer = peers.get(username.as_str());
            let score = score_bundle(&folder, &artist, &album, &files, expected_tracks)
                + peer.map(|resp| peer_score(resp)).unwrap_or(0);

            ManualAlbumCandidate {
                username,
                folder,
                total_size: files.iter().map(|file| file.size).sum(),
                files: files
                    .into_iter()
                    .map(|file| ManualAlbumFile {
                        filename: file.filename,
                        size: file.size,
                        length_secs: file.length,
                        bit_rate: file.bit_rate,
                        extension: Some(file.extension),
                    })
                    .collect(),
                matched_tracks,
                score,
                has_free_upload_slot: peer.is_some_and(|resp| resp.has_free_upload_slot),
                queue_length: peer.map(|resp| resp.queue_length).unwrap_or(0),
            }
        })
        .collect();

    candidates.sort_by_key(|candidate| {
        (
            std::cmp::Reverse(candidate.matched_tracks),
            std::cmp::Reverse(candidate.score),
        )
    });
    candidates.truncate(50);
    candidates
}

/// Pick the file for one track out of a user-chosen album folder. Falls back
/// to a bare track-number match when no filename is plausible — the user
/// vouched for the folder, so odd naming shouldn't block the download.
pub(crate) fn choose_manual_album_file<'a>(
    files: &'a [ManualAlbumFile],
    username: &str,
    ctx: &DownloadTrackContext,
    quality: &Quality,
) -> Option<&'a ManualAlbumFile> {
    let bundle: Vec<AlbumBundleFile> = files
        .iter()
        .map(|file| AlbumBundleFile {
            username: username.to_string(),
            filename: file.filename.clone(),
            size: file.size,
            extension: file.extension.clone().unwrap_or_default(),
            track_number: parse_track_number(&file.filename),
            length: file.length_secs,
            bit_rate: file.bit_rate,
        })
        .collect();

    let chosen_filename =
        pick_manual_bundle_file(&bundle, ctx, quality).map(|file| file.filename.clone())?;

    files.iter().find(|file| file.filename == chosen_filename)
}

/// The pairing logic manual album downloads actually use: plausible match
/// first, then a bare track-number fallback — the user vouched for the
/// folder, so odd naming shouldn't block the download.
fn pick_manual_bundle_file<'a>(
    files: &'a [AlbumBundleFile],
    ctx: &DownloadTrackContext,
    quality: &Quality,
) -> Option<&'a AlbumBundleFile> {
    choose_track_from_bundle(files, ctx, quality).or_else(|| {
        ctx.track_number.and_then(|number| {
            files
                .iter()
                .filter(|file| file.track_number == Some(number))
                .max_by_key(|file| bundle_extension_quality_score(&file.extension, quality))
        })
    })
}

fn choose_track_from_bundle<'a>(
    files: &'a [AlbumBundleFile],
    ctx: &DownloadTrackContext,
    quality: &Quality,
) -> Option<&'a AlbumBundleFile> {
    let plausible = |file: &&AlbumBundleFile| is_plausible_match(&file.filename, file.length, ctx);

    if ctx.track_number.is_some() {
        let numbered = files
            .iter()
            .filter(|file| file.track_number == ctx.track_number)
            .filter(plausible)
            .max_by_key(|file| bundle_extension_quality_score(&file.extension, quality));
        if numbered.is_some() {
            return numbered;
        }
    }

    // Other pressings of the same album order tracks differently, so a title
    // match at a different track number still beats no match at all.
    files
        .iter()
        .filter(plausible)
        .max_by_key(|file| bundle_extension_quality_score(&file.extension, quality))
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
