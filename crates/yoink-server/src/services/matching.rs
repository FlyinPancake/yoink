use std::collections::HashSet;

use chrono::Datelike;
use sea_orm::{ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, EntityTrait, ModelTrait};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    db::{self, match_kind::MatchKind, match_status::MatchStatus, provider::Provider},
    error::{AppError, AppResult},
    providers::{ProviderAlbum, ProviderArtist, ProviderTrack, provider_image_url},
    state::AppState,
    util::{normalize, provider_priority},
};

const ARTIST_CONFIDENCE_MIN: u8 = 86;
const ALBUM_CONFIDENCE_MIN: u8 = 82;
const ALBUM_SEARCH_SCORE_MIN: f64 = 0.65;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ArtistMatchSuggestion {
    pub id: Uuid,
    pub artist_id: Uuid,
    pub left_provider: Provider,
    pub left_external_id: String,
    pub right_provider: Provider,
    pub right_external_id: String,
    pub match_kind: MatchKind,
    pub confidence: u8,
    pub explanation: Option<String>,
    pub external_name: Option<String>,
    pub external_url: Option<String>,
    pub image_url: Option<String>,
    pub disambiguation: Option<String>,
    pub artist_type: Option<String>,
    pub country: Option<String>,
    pub tags: Vec<String>,
    pub popularity: Option<u8>,
    pub status: MatchStatus,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct AlbumMatchSuggestion {
    pub id: Uuid,
    pub album_id: Uuid,
    pub left_provider: Provider,
    pub left_external_id: String,
    pub right_provider: Provider,
    pub right_external_id: String,
    pub match_kind: MatchKind,
    pub confidence: u8,
    pub explanation: Option<String>,
    pub external_name: Option<String>,
    pub external_url: Option<String>,
    pub image_url: Option<String>,
    pub tags: Vec<String>,
    pub popularity: Option<u8>,
    pub status: MatchStatus,
}

fn parse_tags(tags_json: Option<&str>) -> Vec<String> {
    tags_json
        .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
        .unwrap_or_default()
}

fn serialize_tags(tags: &[String]) -> Option<String> {
    if tags.is_empty() {
        None
    } else {
        serde_json::to_string(tags).ok()
    }
}

impl From<db::artist_match_suggestion::Model> for ArtistMatchSuggestion {
    fn from(value: db::artist_match_suggestion::Model) -> Self {
        Self {
            id: value.id,
            artist_id: value.artist_id,
            left_provider: value.left_provider,
            left_external_id: value.left_external_id,
            right_provider: value.right_provider,
            right_external_id: value.right_external_id,
            match_kind: value.match_kind,
            confidence: value.confidence.clamp(0, 100) as u8,
            explanation: value.explanation,
            external_name: value.external_name,
            external_url: value.external_url,
            image_url: value.image_url,
            disambiguation: value.disambiguation,
            artist_type: value.artist_type,
            country: value.country,
            tags: parse_tags(value.tags_json.as_deref()),
            popularity: value.popularity.map(|value| value.clamp(0, 100) as u8),
            status: value.status,
        }
    }
}

impl From<db::artist_match_suggestion::ModelEx> for ArtistMatchSuggestion {
    fn from(value: db::artist_match_suggestion::ModelEx) -> Self {
        db::artist_match_suggestion::Model::from(value).into()
    }
}

impl From<db::album_match_suggestion::Model> for AlbumMatchSuggestion {
    fn from(value: db::album_match_suggestion::Model) -> Self {
        Self {
            id: value.id,
            album_id: value.album_id,
            left_provider: value.left_provider,
            left_external_id: value.left_external_id,
            right_provider: value.right_provider,
            right_external_id: value.right_external_id,
            match_kind: value.match_kind,
            confidence: value.confidence.clamp(0, 100) as u8,
            explanation: value.explanation,
            external_name: value.external_name,
            external_url: value.external_url,
            image_url: value.image_url,
            tags: parse_tags(value.tags_json.as_deref()),
            popularity: value.popularity.map(|value| value.clamp(0, 100) as u8),
            status: value.status,
        }
    }
}

impl From<db::album_match_suggestion::ModelEx> for AlbumMatchSuggestion {
    fn from(value: db::album_match_suggestion::ModelEx) -> Self {
        db::album_match_suggestion::Model::from(value).into()
    }
}

pub(crate) async fn recompute_artist_match_suggestions(
    state: &AppState,
    artist_id: Uuid,
) -> AppResult<()> {
    let artist = db::artist::Entity::find_by_id(artist_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("artist", Some(artist_id.to_string())))?;

    let artist_links = db::artist_provider_link::Entity::find_by_artist(artist_id)
        .all(&state.db)
        .await?;

    let albums: Vec<db::album::Model> = db::album_artist::Entity::find_by_artist(artist_id)
        .find_also_related(db::album::Entity)
        .all(&state.db)
        .await?
        .into_iter()
        .filter_map(|(_, album)| album)
        .collect();

    db::artist_match_suggestion::Entity::delete_pending_for_artist(artist_id)
        .exec(&state.db)
        .await?;

    recompute_artist_level_suggestions(state, artist_id, &artist.name, &artist_links).await?;

    for album in &albums {
        recompute_album_match_suggestions(state, album, &artist.name).await?;
    }

    Ok(())
}

async fn recompute_artist_level_suggestions(
    state: &AppState,
    artist_id: Uuid,
    artist_name: &str,
    artist_links: &[db::artist_provider_link::Model],
) -> AppResult<()> {
    if artist_links.is_empty() {
        return Ok(());
    }

    let existing_pairs: HashSet<(Provider, String)> = artist_links
        .iter()
        .map(|link| (link.provider, link.external_id.clone()))
        .collect();

    let reference = artist_links
        .iter()
        .max_by_key(|link| provider_priority(link.provider))
        .unwrap_or(&artist_links[0]);

    for (provider_id, results) in state.registry.search_artists_all(artist_name).await {
        let Some((candidate, score)) = best_artist_candidate(artist_name, results, |candidate| {
            !existing_pairs.contains(&(provider_id, candidate.external_id.clone()))
        }) else {
            continue;
        };

        let confidence = (score * 100.0).round().clamp(0.0, 100.0) as u8;
        if confidence < ARTIST_CONFIDENCE_MIN {
            continue;
        }

        let suggestion = db::artist_match_suggestion::ActiveModel {
            artist_id: Set(artist_id),
            left_provider: Set(reference.provider),
            left_external_id: Set(reference.external_id.clone()),
            right_provider: Set(provider_id),
            right_external_id: Set(candidate.external_id.clone()),
            match_kind: Set(MatchKind::Fuzzy),
            confidence: Set(i32::from(confidence)),
            explanation: Set(Some(format!("Artist name fuzzy {:.0}%", score * 100.0))),
            external_name: Set(Some(candidate.name)),
            external_url: Set(candidate.url),
            image_url: Set(candidate
                .image_ref
                .as_deref()
                .map(|image_ref| provider_image_url(provider_id, image_ref, 160))),
            disambiguation: Set(candidate.disambiguation),
            artist_type: Set(candidate.artist_type),
            country: Set(candidate.country),
            tags_json: Set(serialize_tags(&candidate.tags)),
            popularity: Set(candidate.popularity.map(i32::from)),
            status: Set(MatchStatus::Pending),
            ..db::artist_match_suggestion::ActiveModel::new()
        };
        suggestion.insert(&state.db).await?;
    }

    Ok(())
}

async fn recompute_album_match_suggestions(
    state: &AppState,
    album: &db::album::Model,
    artist_name: &str,
) -> AppResult<()> {
    db::album_match_suggestion::Entity::delete_pending_for_album(album.id)
        .exec(&state.db)
        .await?;

    let existing_links = album
        .find_related(db::album_provider_link::Entity)
        .all(&state.db)
        .await?;

    if existing_links.is_empty() {
        return Ok(());
    }

    let metadata_links: Vec<_> = existing_links
        .into_iter()
        .filter(|link| state.registry.metadata_provider(link.provider).is_some())
        .collect();

    if metadata_links.is_empty() {
        return Ok(());
    }

    let existing_pairs: HashSet<(Provider, String)> = metadata_links
        .iter()
        .map(|link| (link.provider, link.provider_album_id.clone()))
        .collect();

    let (reference_provider, reference_tracks) = pick_reference_tracks(state, &metadata_links)
        .await
        .unwrap_or_else(|| (metadata_links[0].provider, Vec::new()));

    let reference_pair = metadata_links
        .iter()
        .find(|link| link.provider == reference_provider)
        .map(|link| (link.provider, link.provider_album_id.clone()))
        .unwrap_or_else(|| {
            (
                metadata_links[0].provider,
                metadata_links[0].provider_album_id.clone(),
            )
        });

    for (provider_id, artists) in state.registry.search_artists_all(artist_name).await {
        let Some(provider) = state.registry.metadata_provider(provider_id) else {
            continue;
        };

        let Some((candidate_album, album_score)) =
            best_album_candidate(provider.as_ref(), artists, album).await
        else {
            continue;
        };

        if existing_pairs.contains(&(provider_id, candidate_album.external_id.clone())) {
            continue;
        }

        let target_tracks = provider
            .fetch_tracks(&candidate_album.external_id)
            .await
            .map(|(tracks, _)| tracks)
            .unwrap_or_default();

        let isrc_overlap = count_isrc_overlap(&reference_tracks, &target_tracks);
        let title_overlap = track_title_overlap(&reference_tracks, &target_tracks);

        let (match_kind, confidence, explanation) = if isrc_overlap > 0 {
            let confidence = (90 + (isrc_overlap as u8).saturating_mul(3)).min(100);
            (
                MatchKind::IsrcExact,
                confidence,
                Some(format!("ISRC overlap: {isrc_overlap} track(s)")),
            )
        } else {
            let combined = ((album_score * 0.7) + (title_overlap * 0.3)) * 100.0;
            let confidence = combined.round().clamp(0.0, 100.0) as u8;
            if confidence < ALBUM_CONFIDENCE_MIN {
                continue;
            }
            (
                MatchKind::Fuzzy,
                confidence,
                Some(format!(
                    "Fuzzy match album={:.0}% track={:.0}%",
                    album_score * 100.0,
                    title_overlap * 100.0,
                )),
            )
        };

        let suggestion = db::album_match_suggestion::ActiveModel {
            album_id: Set(album.id),
            left_provider: Set(reference_pair.0),
            left_external_id: Set(reference_pair.1.clone()),
            right_provider: Set(provider_id),
            right_external_id: Set(candidate_album.external_id.clone()),
            match_kind: Set(match_kind),
            confidence: Set(i32::from(confidence)),
            explanation: Set(explanation),
            external_name: Set(Some(candidate_album.title)),
            external_url: Set(candidate_album.url),
            image_url: Set(candidate_album
                .cover_ref
                .as_deref()
                .map(|image_ref| provider_image_url(provider_id, image_ref, 160))),
            tags_json: Set(None),
            popularity: Set(None),
            status: Set(MatchStatus::Pending),
            ..db::album_match_suggestion::ActiveModel::new()
        };
        suggestion.insert(&state.db).await?;
    }

    Ok(())
}

async fn pick_reference_tracks(
    state: &AppState,
    links: &[db::album_provider_link::Model],
) -> Option<(Provider, Vec<ProviderTrack>)> {
    let mut sorted: Vec<&db::album_provider_link::Model> = links.iter().collect();
    sorted.sort_by_key(|link| std::cmp::Reverse(provider_priority(link.provider)));

    for link in sorted {
        let provider = state.registry.metadata_provider(link.provider)?;
        if let Ok((tracks, _)) = provider.fetch_tracks(&link.provider_album_id).await
            && !tracks.is_empty()
        {
            return Some((link.provider, tracks));
        }
    }

    None
}

async fn best_album_candidate(
    provider: &dyn crate::providers::MetadataProvider,
    artists: Vec<ProviderArtist>,
    local_album: &db::album::Model,
) -> Option<(ProviderAlbum, f64)> {
    let local_title = normalize(&local_album.title);
    let local_year = local_album.release_date.map(|date| date.year());

    let mut best: Option<(ProviderAlbum, f64)> = None;

    for artist in artists.into_iter().take(4) {
        let Ok(albums) = provider.fetch_albums(&artist.external_id).await else {
            continue;
        };

        for album in albums {
            let candidate_title = normalize(&album.title);
            let mut score = strsim::jaro_winkler(&local_title, &candidate_title);

            if let (Some(local_year), Some(candidate_date)) = (local_year, album.release_date)
                && candidate_date.year() == local_year
            {
                score = (score + 0.08).min(1.0);
            }

            if best.as_ref().is_none_or(|(_, current)| score > *current) {
                best = Some((album, score));
            }
        }
    }

    best.filter(|(_, score)| *score >= ALBUM_SEARCH_SCORE_MIN)
}

fn best_artist_candidate(
    local_artist_name: &str,
    candidates: Vec<ProviderArtist>,
    mut predicate: impl FnMut(&ProviderArtist) -> bool,
) -> Option<(ProviderArtist, f64)> {
    let local = normalize(local_artist_name);

    candidates
        .into_iter()
        .filter(|candidate| predicate(candidate))
        .map(|candidate| {
            let score = strsim::jaro_winkler(&local, &normalize(&candidate.name));
            (candidate, score)
        })
        .max_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .and_then(|best| {
            if (best.1 * 100.0).round() >= f64::from(ARTIST_CONFIDENCE_MIN) {
                Some(best)
            } else {
                None
            }
        })
}

fn count_isrc_overlap(left: &[ProviderTrack], right: &[ProviderTrack]) -> usize {
    let left_isrc: HashSet<String> = left
        .iter()
        .filter_map(|track| track.isrc.as_ref())
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty())
        .collect();

    if left_isrc.is_empty() {
        return 0;
    }

    right
        .iter()
        .filter_map(|track| track.isrc.as_ref())
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty())
        .filter(|isrc| left_isrc.contains(isrc))
        .count()
}

fn track_title_overlap(left: &[ProviderTrack], right: &[ProviderTrack]) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let left_titles: Vec<String> = left.iter().map(|track| normalize(&track.title)).collect();
    let right_titles: Vec<String> = right.iter().map(|track| normalize(&track.title)).collect();

    let mut total = 0.0;
    for left_title in &left_titles {
        let best = right_titles
            .iter()
            .map(|right_title| strsim::jaro_winkler(left_title, right_title))
            .fold(0.0, f64::max);
        total += best;
    }

    total / left_titles.len() as f64
}

pub(crate) async fn primary_artist_id_for_album(
    state: &AppState,
    album_id: Uuid,
) -> AppResult<Option<Uuid>> {
    Ok(db::album_artist::Entity::find_by_album_ordered(album_id)
        .one(&state.db)
        .await?
        .map(|junction| junction.artist_id))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::Value;

    use super::*;

    fn track(title: &str, isrc: Option<&str>) -> ProviderTrack {
        ProviderTrack {
            external_id: title.to_string(),
            title: title.to_string(),
            version: None,
            track_number: 1,
            disc_number: Some(1),
            duration_secs: 180,
            isrc: isrc.map(str::to_string),
            explicit: false,
            extra: HashMap::<String, Value>::new(),
        }
    }

    fn artist(name: &str, external_id: &str) -> ProviderArtist {
        ProviderArtist {
            external_id: external_id.to_string(),
            name: name.to_string(),
            image_ref: None,
            url: None,
            disambiguation: None,
            artist_type: None,
            country: None,
            tags: Vec::new(),
            popularity: None,
        }
    }

    #[test]
    fn best_artist_candidate_prefers_highest_fuzzy_match() {
        let result = best_artist_candidate(
            "Radiohead",
            vec![
                artist("Radio Head Tribute", "1"),
                artist("Radiohead", "2"),
                artist("Rdiohead", "3"),
            ],
            |_| true,
        );

        let (candidate, _score) = result.expect("expected candidate");
        assert_eq!(candidate.external_id, "2");
    }

    #[test]
    fn count_isrc_overlap_matches_case_insensitively() {
        let left = vec![track("A", Some("usabc123")), track("B", Some("usxyz987"))];
        let right = vec![track("C", Some("USABC123")), track("D", Some("nomatch"))];

        assert_eq!(count_isrc_overlap(&left, &right), 1);
    }

    #[test]
    fn track_title_overlap_returns_zero_for_empty_side() {
        assert_eq!(track_title_overlap(&[], &[track("Song", None)]), 0.0);
        assert_eq!(track_title_overlap(&[track("Song", None)], &[]), 0.0);
    }
}
