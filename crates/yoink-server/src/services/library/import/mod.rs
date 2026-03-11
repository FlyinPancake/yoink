use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Utc;
use uuid::Uuid;

use yoink_shared::{ImportAlbumCandidate, ImportMatchStatus, ImportPreviewItem, MonitoredAlbum};

use crate::{db, error::AppResult, models::MonitoredArtist, state::AppState};

use super::{normalize_text, parse_release_year, update_wanted};

mod external;
mod local;

pub(crate) use external::{confirm_external_import, preview_external_import};
pub(crate) use local::{confirm_import_library, preview_import_library, scan_and_import_library};

#[derive(Debug, Clone)]
struct LocalAlbumDir {
    artist_name: String,
    album_title: String,
    year: Option<String>,
}

#[derive(Debug, Clone)]
struct ExternalAlbumDir {
    artist_name: String,
    album_title: String,
    year: Option<String>,
    source_dir: PathBuf,
    audio_files: Vec<PathBuf>,
}

fn match_album_candidates(
    discovered_artist: &str,
    discovered_album: &str,
    discovered_year: Option<&str>,
    artist_names_lower: &HashMap<String, (Uuid, String)>,
    albums: &[MonitoredAlbum],
) -> (Vec<ImportAlbumCandidate>, ImportMatchStatus) {
    let needle = normalize_text(discovered_artist);
    let mut candidates = Vec::new();
    let mut match_status = ImportMatchStatus::Unmatched;

    if let Some((artist_id, artist_name)) = artist_names_lower.get(&needle) {
        let target_title = normalize_text(discovered_album);

        for album in albums
            .iter()
            .filter(|a| a.artist_id == *artist_id || a.artist_ids.contains(artist_id))
        {
            let album_title_norm = normalize_text(&album.title);
            let album_year = album.release_date.as_deref().and_then(parse_release_year);

            let title_match = album_title_norm == target_title;
            let year_match = match (discovered_year, &album_year) {
                (Some(ly), Some(ay)) => ly == ay,
                (None, _) | (_, None) => true,
            };

            if title_match && year_match {
                let confidence = if discovered_year.is_some() && album_year.is_some() {
                    100
                } else {
                    85
                };
                candidates.push(ImportAlbumCandidate {
                    album_id: Some(album.id),
                    artist_id: *artist_id,
                    artist_name: artist_name.clone(),
                    album_title: album.title.clone(),
                    release_date: album.release_date.clone(),
                    cover_url: album.cover_url.clone(),
                    album_type: album.album_type.clone(),
                    explicit: album.explicit,
                    monitored: album.monitored,
                    acquired: album.acquired,
                    confidence,
                });
            } else if title_match {
                candidates.push(ImportAlbumCandidate {
                    album_id: Some(album.id),
                    artist_id: *artist_id,
                    artist_name: artist_name.clone(),
                    album_title: album.title.clone(),
                    release_date: album.release_date.clone(),
                    cover_url: album.cover_url.clone(),
                    album_type: album.album_type.clone(),
                    explicit: album.explicit,
                    monitored: album.monitored,
                    acquired: album.acquired,
                    confidence: 70,
                });
            } else {
                let sim = strsim::jaro_winkler(&target_title, &album_title_norm);
                if sim > 0.85 {
                    let confidence = (sim * 80.0) as u8;
                    candidates.push(ImportAlbumCandidate {
                        album_id: Some(album.id),
                        artist_id: *artist_id,
                        artist_name: artist_name.clone(),
                        album_title: album.title.clone(),
                        release_date: album.release_date.clone(),
                        cover_url: album.cover_url.clone(),
                        album_type: album.album_type.clone(),
                        explicit: album.explicit,
                        monitored: album.monitored,
                        acquired: album.acquired,
                        confidence,
                    });
                }
            }
        }

        if candidates.is_empty() {
            candidates.push(ImportAlbumCandidate {
                album_id: None,
                artist_id: *artist_id,
                artist_name: artist_name.clone(),
                album_title: discovered_album.to_string(),
                release_date: discovered_year.map(String::from),
                cover_url: None,
                album_type: None,
                explicit: false,
                monitored: false,
                acquired: false,
                confidence: 50,
            });
            match_status = ImportMatchStatus::Partial;
        }
    }

    candidates.sort_by(|a, b| b.confidence.cmp(&a.confidence));

    if !candidates.is_empty() && match_status != ImportMatchStatus::Partial {
        if candidates[0].confidence >= 85 {
            match_status = ImportMatchStatus::Matched;
        } else {
            match_status = ImportMatchStatus::Partial;
        }
    }

    (candidates, match_status)
}

fn sort_preview_items(items: &mut [ImportPreviewItem]) {
    items.sort_by(|a, b| {
        let rank = |item: &ImportPreviewItem| -> u8 {
            if item.already_imported {
                return 3;
            }
            match item.match_status {
                ImportMatchStatus::Unmatched => 0,
                ImportMatchStatus::Partial => 1,
                ImportMatchStatus::Matched => 2,
            }
        };
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.discovered_artist.cmp(&b.discovered_artist))
            .then_with(|| a.discovered_album.cmp(&b.discovered_album))
    });
}

async fn ensure_monitored_artist(
    state: &AppState,
    artist_name: &str,
) -> AppResult<Option<(Uuid, bool)>> {
    let needle = normalize_text(artist_name);
    {
        let artists = state.monitored_artists.read().await;
        if let Some(artist) = artists
            .iter()
            .find(|a| normalize_text(&a.name) == needle)
            .cloned()
        {
            return Ok(Some((artist.id, false)));
        }
    }

    let all_results = state.registry.search_artists_all(artist_name).await;

    let mut best_match: Option<(String, crate::providers::ProviderArtist)> = None;
    for (provider_id, candidates) in &all_results {
        if let Some(exact) = candidates
            .iter()
            .find(|a| normalize_text(&a.name) == needle)
            .cloned()
        {
            best_match = Some((provider_id.clone(), exact));
            break;
        }
        if best_match.is_none()
            && let Some(first) = candidates.first().cloned()
        {
            best_match = Some((provider_id.clone(), first));
        }
    }

    let Some((provider_id, artist)) = best_match else {
        return Ok(None);
    };

    let new_id = Uuid::now_v7();
    let image_url = artist
        .image_ref
        .as_deref()
        .map(|r| yoink_shared::provider_image_url(&provider_id, r, 640));

    let monitored = MonitoredArtist {
        id: new_id,
        name: artist.name.clone(),
        image_url,
        bio: None,
        monitored: true,
        added_at: Utc::now(),
    };
    db::upsert_artist(&state.db, &monitored).await?;

    let link = db::ArtistProviderLink {
        id: Uuid::now_v7(),
        artist_id: new_id,
        provider: provider_id,
        external_id: artist.external_id,
        external_url: artist.url,
        external_name: Some(artist.name),
        image_ref: artist.image_ref,
    };
    db::upsert_artist_provider_link(&state.db, &link).await?;

    {
        let mut artists = state.monitored_artists.write().await;
        if artists.iter().all(|a| a.id != monitored.id) {
            artists.push(monitored.clone());
        }
    }

    Ok(Some((new_id, true)))
}

async fn import_local_album(
    state: &AppState,
    artist_id: Uuid,
    album_title: &str,
    year_hint: Option<&str>,
) -> AppResult<bool> {
    let target_title = normalize_text(album_title);
    let mut albums = state.monitored_albums.write().await;
    let hint_year = year_hint.and_then(parse_release_year);

    let mut matched_index = None;
    for (idx, album) in albums.iter().enumerate() {
        if album.artist_id != artist_id && !album.artist_ids.contains(&artist_id) {
            continue;
        }
        if normalize_text(&album.title) != target_title {
            continue;
        }
        if let Some(ref year) = hint_year {
            let album_year = album.release_date.as_deref().and_then(parse_release_year);
            if album_year.as_ref() != Some(year) {
                continue;
            }
        }
        matched_index = Some(idx);
        break;
    }

    let Some(idx) = matched_index else {
        return Ok(false);
    };

    let album = &mut albums[idx];
    let mut changed = false;
    if !album.monitored {
        album.monitored = true;
        changed = true;
    }
    if !album.acquired {
        album.acquired = true;
        changed = true;
    }
    update_wanted(album);
    if changed {
        db::update_album_flags(
            &state.db,
            album.id,
            album.monitored,
            album.acquired,
            album.wanted,
        )
        .await?;
    }
    Ok(changed)
}

fn md5_hash(input: &str) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

fn split_album_folder_name(name: &str) -> (String, Option<String>) {
    if let Some((title, tail)) = name.rsplit_once(" (") {
        let inner = tail.trim_end_matches(')').trim();
        let year_str = inner.split('-').next().unwrap_or("");
        if year_str.len() == 4 && year_str.chars().all(|c| c.is_ascii_digit()) {
            return (title.trim().to_string(), Some(year_str.to_string()));
        }
    }
    (name.trim().to_string(), None)
}
