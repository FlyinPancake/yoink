use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use sea_orm::{ActiveEnum, ColumnTrait, EntityTrait, QueryFilter};
use yoink_shared::ImportAlbumCandidate;

use crate::{
    db::{album, album_artist, artist, track, wanted_status::WantedStatus},
    error::AppResult,
    state::AppState,
    util::normalize,
};

use super::types::{
    ALBUM_CANDIDATE_MIN_CONFIDENCE, ARTIST_CANDIDATE_MIN_CONFIDENCE, DiscoveredAlbum,
    LocalArtistCatalog, STRONG_FUZZY_MATCH_SCORE,
};

pub(super) fn build_candidates(
    discovered: &DiscoveredAlbum,
    catalog: &[LocalArtistCatalog],
) -> Vec<ImportAlbumCandidate> {
    let artist_name = normalize(&discovered.discovered_artist);
    let album_name = normalize(&discovered.discovered_album);

    let mut candidates = Vec::new();

    for entry in catalog {
        let candidate_artist_name = normalize(&entry.artist.name);
        let artist_score = strsim::jaro_winkler(&artist_name, &candidate_artist_name);
        let artist_confidence = confidence_percent(artist_score);

        if artist_confidence < ARTIST_CANDIDATE_MIN_CONFIDENCE
            || !is_plausible_name_match(&artist_name, &candidate_artist_name, artist_score)
        {
            continue;
        }

        let mut added_album_for_artist = false;

        for album in &entry.albums {
            let candidate_album_name = normalize(&album.title);
            let album_score = strsim::jaro_winkler(&album_name, &candidate_album_name);

            if !is_plausible_name_match(&album_name, &candidate_album_name, album_score) {
                continue;
            }

            let mut combined = (artist_score * 0.45) + (album_score * 0.55);

            if let (Some(discovered_year), Some(release_date)) =
                (discovered.discovered_year.as_deref(), album.release_date)
                && discovered_year == release_date.format("%Y").to_string()
            {
                combined = (combined + 0.08).min(1.0);
            }

            let confidence = confidence_percent(combined);
            if confidence < ALBUM_CANDIDATE_MIN_CONFIDENCE {
                continue;
            }

            added_album_for_artist = true;
            candidates.push(ImportAlbumCandidate {
                album_id: Some(album.id),
                artist_id: entry.artist.id,
                artist_name: entry.artist.name.clone(),
                album_title: album.title.clone(),
                release_date: album.release_date.map(|date| date.to_string()),
                cover_url: album.cover_url.clone(),
                album_type: Some(album.album_type.to_value()),
                explicit: album.explicit,
                monitored: album.wanted_status != WantedStatus::Unmonitored,
                acquired: album.wanted_status == WantedStatus::Acquired,
                confidence,
            });
        }

        if !added_album_for_artist {
            candidates.push(ImportAlbumCandidate {
                album_id: None,
                artist_id: entry.artist.id,
                artist_name: entry.artist.name.clone(),
                album_title: discovered.discovered_album.clone(),
                release_date: discovered
                    .discovered_year
                    .as_ref()
                    .map(|year| format!("{year}-01-01")),
                cover_url: None,
                album_type: None,
                explicit: false,
                monitored: entry.artist.monitored,
                acquired: false,
                confidence: artist_confidence,
            });
        }
    }

    dedupe_candidates(&mut candidates);
    candidates.sort_by(|left, right| {
        right
            .confidence
            .cmp(&left.confidence)
            .then_with(|| left.artist_name.cmp(&right.artist_name))
            .then_with(|| left.album_title.cmp(&right.album_title))
    });
    candidates.truncate(5);
    candidates
}

pub(super) async fn load_local_artist_catalog(
    state: &AppState,
) -> AppResult<Vec<LocalArtistCatalog>> {
    let artists = artist::Entity::find().all(&state.db).await?;
    let artist_ids: HashSet<_> = artists.iter().map(|artist| artist.id).collect();

    let mut albums_by_artist: HashMap<uuid::Uuid, Vec<album::Model>> = HashMap::new();
    let album_links = album_artist::Entity::find()
        .find_also_related(album::Entity)
        .all(&state.db)
        .await?;

    for (junction, album) in album_links {
        if let Some(album) = album
            && artist_ids.contains(&junction.artist_id)
        {
            albums_by_artist
                .entry(junction.artist_id)
                .or_default()
                .push(album);
        }
    }

    Ok(artists
        .into_iter()
        .map(|artist| LocalArtistCatalog {
            albums: albums_by_artist.remove(&artist.id).unwrap_or_default(),
            artist,
        })
        .collect())
}

pub(super) async fn load_imported_paths(
    state: &AppState,
    root_path: &Path,
) -> AppResult<HashSet<String>> {
    if root_path != state.music_root {
        return Ok(HashSet::new());
    }

    Ok(track::Entity::find()
        .filter(track::Column::FilePath.is_not_null())
        .all(&state.db)
        .await?
        .into_iter()
        .filter_map(|track| track.file_path)
        .collect())
}

pub(super) fn is_album_already_imported(
    album: &DiscoveredAlbum,
    imported_paths: &HashSet<String>,
    root_path: &Path,
) -> bool {
    if imported_paths.is_empty() {
        return false;
    }

    let relative_paths = album
        .files
        .iter()
        .filter_map(|file| {
            file.absolute_path
                .strip_prefix(root_path)
                .ok()
                .map(|path| path.to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();

    !relative_paths.is_empty()
        && relative_paths
            .iter()
            .all(|path| imported_paths.contains(path))
}

fn is_plausible_name_match(left: &str, right: &str, score: f64) -> bool {
    left == right || has_meaningful_token_overlap(left, right) || score >= STRONG_FUZZY_MATCH_SCORE
}

fn has_meaningful_token_overlap(left: &str, right: &str) -> bool {
    let left_tokens: HashSet<&str> = left
        .split_whitespace()
        .filter(|token| token.len() >= 3)
        .collect();
    let right_tokens: HashSet<&str> = right
        .split_whitespace()
        .filter(|token| token.len() >= 3)
        .collect();

    !left_tokens.is_empty() && left_tokens.iter().any(|token| right_tokens.contains(token))
}

fn dedupe_candidates(candidates: &mut Vec<ImportAlbumCandidate>) {
    let mut seen = HashSet::new();
    candidates.retain(|candidate| {
        let key = (
            candidate.artist_id,
            candidate.album_id,
            normalize(&candidate.artist_name),
            normalize(&candidate.album_title),
        );
        seen.insert(key)
    });
}

fn confidence_percent(score: f64) -> u8 {
    (score * 100.0).round().clamp(0.0, 100.0) as u8
}
