use std::collections::{HashMap, HashSet};

use chrono::Utc;
use uuid::Uuid;

use crate::{
    db,
    error::{AppError, AppResult},
    models::MonitoredAlbum,
    providers::{ProviderAlbum, ProviderArtist, ProviderTrack},
    state::AppState,
    util::{normalize, provider_priority},
};

pub(crate) async fn recompute_artist_match_suggestions(
    state: &AppState,
    artist_id: Uuid,
) -> AppResult<()> {
    let artist_name = {
        let artists = state.monitored_artists.read().await;
        artists
            .iter()
            .find(|a| a.id == artist_id)
            .map(|a| a.name.clone())
            .ok_or_else(|| AppError::not_found("artist", Some(artist_id.to_string())))?
    };

    let albums: Vec<MonitoredAlbum> = {
        let albums = state.monitored_albums.read().await;
        albums
            .iter()
            .filter(|a| a.artist_id == artist_id)
            .cloned()
            .collect()
    };

    let artist_links = db::load_artist_provider_links(&state.db, artist_id).await?;

    db::clear_pending_match_suggestions(&state.db, "artist", artist_id).await?;

    recompute_artist_level_suggestions(state, artist_id, &artist_name, &artist_links).await?;

    for album in albums {
        recompute_album_match_suggestions(state, &album, &artist_name).await?;
    }

    Ok(())
}

async fn recompute_artist_level_suggestions(
    state: &AppState,
    artist_id: Uuid,
    artist_name: &str,
    artist_links: &[db::ArtistProviderLink],
) -> AppResult<()> {
    if artist_links.is_empty() {
        return Ok(());
    }

    let existing_pairs: HashSet<(String, String)> = artist_links
        .iter()
        .map(|l| (l.provider.clone(), l.external_id.clone()))
        .collect();

    let reference = artist_links
        .iter()
        .max_by_key(|l| provider_priority(&l.provider))
        .unwrap_or(&artist_links[0]);

    for provider_id in state.registry.metadata_provider_ids() {
        let Some(provider) = state.registry.metadata_provider(&provider_id) else {
            continue;
        };

        let results = provider
            .search_artists(artist_name)
            .await
            .unwrap_or_default();
        let Some((candidate, score)) = best_artist_candidate(artist_name, results, |candidate| {
            !existing_pairs.contains(&(provider_id.clone(), candidate.external_id.clone()))
        }) else {
            continue;
        };

        let confidence = (score * 100.0).round().clamp(0.0, 100.0) as u8;
        if confidence < 86 {
            continue;
        }

        let now = Utc::now();
        let suggestion = db::MatchSuggestion {
            id: Uuid::now_v7(),
            scope_type: "artist".to_string(),
            scope_id: artist_id,
            left_provider: reference.provider.clone(),
            left_external_id: reference.external_id.clone(),
            right_provider: provider_id.clone(),
            right_external_id: candidate.external_id.clone(),
            match_kind: "fuzzy".to_string(),
            confidence,
            explanation: Some(format!("Artist name fuzzy {:.0}%", score * 100.0)),
            external_name: Some(candidate.name),
            external_url: candidate.url,
            image_ref: candidate.image_ref,
            disambiguation: candidate.disambiguation,
            artist_type: candidate.artist_type,
            country: candidate.country,
            tags: candidate.tags,
            popularity: candidate.popularity,
            status: "pending".to_string(),
            created_at: now,
            updated_at: now,
        };

        db::upsert_match_suggestion(&state.db, &suggestion).await?;
    }

    Ok(())
}

async fn recompute_album_match_suggestions(
    state: &AppState,
    album: &MonitoredAlbum,
    artist_name: &str,
) -> AppResult<()> {
    let existing_links = db::load_album_provider_links(&state.db, album.id).await?;

    db::clear_pending_match_suggestions(&state.db, "album", album.id).await?;

    if existing_links.is_empty() {
        return Ok(());
    }

    let metadata_links: Vec<_> = existing_links
        .iter()
        .filter(|l| state.registry.metadata_provider(&l.provider).is_some())
        .collect();

    if metadata_links.is_empty() {
        return Ok(());
    }

    let mut link_by_provider: HashMap<String, (String, String)> = HashMap::new();
    for l in &metadata_links {
        link_by_provider.insert(
            l.provider.clone(),
            (l.provider.clone(), l.external_id.clone()),
        );
    }

    let existing_pairs: HashSet<(String, String)> = metadata_links
        .iter()
        .map(|l| (l.provider.clone(), l.external_id.clone()))
        .collect();

    let (reference_provider, reference_tracks) = pick_reference_tracks(state, &metadata_links)
        .await
        .unwrap_or_else(|| (metadata_links[0].provider.clone(), Vec::new()));

    let reference_pair = link_by_provider
        .get(&reference_provider)
        .cloned()
        .unwrap_or_else(|| {
            (
                metadata_links[0].provider.clone(),
                metadata_links[0].external_id.clone(),
            )
        });

    for provider_id in state.registry.metadata_provider_ids() {
        let Some(provider) = state.registry.metadata_provider(&provider_id) else {
            continue;
        };

        let artists = provider
            .search_artists(artist_name)
            .await
            .unwrap_or_default();

        let candidates = best_album_candidates(provider.as_ref(), artists, album).await;
        let Some((candidate_album, album_score)) = candidates else {
            continue;
        };

        if existing_pairs.contains(&(provider_id.clone(), candidate_album.external_id.clone())) {
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
                "isrc_exact".to_string(),
                confidence,
                Some(format!("ISRC overlap: {isrc_overlap} track(s)")),
            )
        } else {
            let combined = ((album_score * 0.7) + (title_overlap * 0.3)) * 100.0;
            let confidence = combined.round().clamp(0.0, 100.0) as u8;
            if confidence < 82 {
                continue;
            }
            (
                "fuzzy".to_string(),
                confidence,
                Some(format!(
                    "Fuzzy match album={:.0}% track={:.0}%",
                    album_score * 100.0,
                    title_overlap * 100.0
                )),
            )
        };

        let now = Utc::now();
        let suggestion = db::MatchSuggestion {
            id: Uuid::now_v7(),
            scope_type: "album".to_string(),
            scope_id: album.id,
            left_provider: reference_pair.0.clone(),
            left_external_id: reference_pair.1.clone(),
            right_provider: provider_id.clone(),
            right_external_id: candidate_album.external_id.clone(),
            match_kind,
            confidence,
            explanation,
            external_name: Some(candidate_album.title.clone()),
            external_url: candidate_album.url.clone(),
            image_ref: candidate_album.cover_ref.clone(),
            disambiguation: None,
            artist_type: None,
            country: None,
            tags: Vec::new(),
            popularity: None,
            status: "pending".to_string(),
            created_at: now,
            updated_at: now,
        };

        db::upsert_match_suggestion(&state.db, &suggestion).await?;
    }

    Ok(())
}

async fn pick_reference_tracks(
    state: &AppState,
    links: &[&db::AlbumProviderLink],
) -> Option<(String, Vec<ProviderTrack>)> {
    let mut sorted: Vec<&db::AlbumProviderLink> = links.to_vec();
    sorted.sort_by_key(|l| std::cmp::Reverse(provider_priority(&l.provider)));

    for link in sorted {
        let provider = state.registry.metadata_provider(&link.provider)?;
        if let Ok((tracks, _)) = provider.fetch_tracks(&link.external_id).await
            && !tracks.is_empty()
        {
            return Some((link.provider.clone(), tracks));
        }
    }

    None
}

async fn best_album_candidates(
    provider: &dyn crate::providers::MetadataProvider,
    artists: Vec<ProviderArtist>,
    local_album: &MonitoredAlbum,
) -> Option<(ProviderAlbum, f64)> {
    let local_title = normalize(&local_album.title);
    let local_year = local_album
        .release_date
        .as_deref()
        .and_then(|d| d.split('-').next())
        .map(|s| s.to_string());

    let mut best: Option<(ProviderAlbum, f64)> = None;

    for artist in artists.into_iter().take(4) {
        let albums = provider.fetch_albums(&artist.external_id).await.ok()?;
        for album in albums {
            let candidate_title = normalize(&album.title);
            let mut score = strsim::jaro_winkler(&local_title, &candidate_title);

            if let (Some(local_year), Some(candidate_date)) =
                (&local_year, album.release_date.as_deref())
                && candidate_date.starts_with(local_year)
            {
                score = (score + 0.08).min(1.0);
            }

            if best.as_ref().is_none_or(|(_, s)| score > *s) {
                best = Some((album, score));
            }
        }
    }

    best.filter(|(_, score)| *score >= 0.65)
}

fn count_isrc_overlap(left: &[ProviderTrack], right: &[ProviderTrack]) -> usize {
    let left_isrc: HashSet<String> = left
        .iter()
        .filter_map(|t| t.isrc.as_ref())
        .map(|v| v.trim().to_ascii_uppercase())
        .filter(|v| !v.is_empty())
        .collect();

    if left_isrc.is_empty() {
        return 0;
    }

    right
        .iter()
        .filter_map(|t| t.isrc.as_ref())
        .map(|v| v.trim().to_ascii_uppercase())
        .filter(|v| !v.is_empty())
        .filter(|isrc| left_isrc.contains(isrc))
        .count()
}

fn track_title_overlap(left: &[ProviderTrack], right: &[ProviderTrack]) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let left_titles: Vec<String> = left.iter().map(|t| normalize(&t.title)).collect();
    let right_titles: Vec<String> = right.iter().map(|t| normalize(&t.title)).collect();

    let mut total = 0.0;
    let mut matched = 0usize;
    for lt in &left_titles {
        let best = right_titles
            .iter()
            .map(|rt| strsim::jaro_winkler(lt, rt))
            .fold(0.0, f64::max);
        total += best;
        matched += 1;
    }

    if matched == 0 {
        0.0
    } else {
        total / matched as f64
    }
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
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .and_then(|best| if best.1 >= 0.70 { Some(best) } else { None })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn make_provider_artist(name: &str) -> ProviderArtist {
        ProviderArtist {
            external_id: format!("ext_{}", name.to_lowercase().replace(' ', "_")),
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

    fn make_provider_track(title: &str, isrc: Option<&str>) -> ProviderTrack {
        ProviderTrack {
            external_id: format!("track_{}", title.to_lowercase().replace(' ', "_")),
            title: title.to_string(),
            version: None,
            track_number: 1,
            disc_number: None,
            duration_secs: 200,
            isrc: isrc.map(|s| s.to_string()),
            artists: None,
            explicit: false,
            extra: HashMap::new(),
        }
    }

    // ── normalize ───────────────────────────────────────────────

    #[test]
    fn normalize_lowercases() {
        assert_eq!(normalize("HELLO WORLD"), "hello world");
    }

    #[test]
    fn normalize_non_alphanumeric_to_space() {
        assert_eq!(normalize("hello-world"), "hello world");
        assert_eq!(normalize("hello.world!"), "hello world");
    }

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize("hello   world"), "hello world");
        assert_eq!(normalize("  spaced  out  "), "spaced out");
    }

    #[test]
    fn normalize_mixed() {
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

    // ── count_isrc_overlap ──────────────────────────────────────

    #[test]
    fn isrc_overlap_full_match() {
        let left = vec![
            make_provider_track("Track 1", Some("ISRC001")),
            make_provider_track("Track 2", Some("ISRC002")),
        ];
        let right = vec![
            make_provider_track("Track A", Some("ISRC001")),
            make_provider_track("Track B", Some("ISRC002")),
        ];
        assert_eq!(count_isrc_overlap(&left, &right), 2);
    }

    #[test]
    fn isrc_overlap_partial() {
        let left = vec![
            make_provider_track("Track 1", Some("ISRC001")),
            make_provider_track("Track 2", Some("ISRC002")),
        ];
        let right = vec![
            make_provider_track("Track A", Some("ISRC001")),
            make_provider_track("Track B", Some("ISRC999")),
        ];
        assert_eq!(count_isrc_overlap(&left, &right), 1);
    }

    #[test]
    fn isrc_overlap_none() {
        let left = vec![make_provider_track("Track 1", Some("ISRC001"))];
        let right = vec![make_provider_track("Track A", Some("ISRC999"))];
        assert_eq!(count_isrc_overlap(&left, &right), 0);
    }

    #[test]
    fn isrc_overlap_empty_left() {
        let right = vec![make_provider_track("Track A", Some("ISRC001"))];
        assert_eq!(count_isrc_overlap(&[], &right), 0);
    }

    #[test]
    fn isrc_overlap_no_isrc_on_left() {
        let left = vec![make_provider_track("Track 1", None)];
        let right = vec![make_provider_track("Track A", Some("ISRC001"))];
        assert_eq!(count_isrc_overlap(&left, &right), 0);
    }

    #[test]
    fn isrc_overlap_case_insensitive() {
        let left = vec![make_provider_track("Track 1", Some("isrc001"))];
        let right = vec![make_provider_track("Track A", Some("ISRC001"))];
        assert_eq!(count_isrc_overlap(&left, &right), 1);
    }

    #[test]
    fn isrc_overlap_ignores_whitespace() {
        let left = vec![make_provider_track("Track 1", Some(" ISRC001 "))];
        let right = vec![make_provider_track("Track A", Some("ISRC001"))];
        assert_eq!(count_isrc_overlap(&left, &right), 1);
    }

    // ── track_title_overlap ─────────────────────────────────────

    #[test]
    fn title_overlap_identical() {
        let left = vec![
            make_provider_track("Hello", None),
            make_provider_track("World", None),
        ];
        let right = vec![
            make_provider_track("Hello", None),
            make_provider_track("World", None),
        ];
        let score = track_title_overlap(&left, &right);
        assert!(score > 0.95, "Expected near-1.0, got {score}");
    }

    #[test]
    fn title_overlap_completely_different() {
        let left = vec![make_provider_track("AAAAAA", None)];
        let right = vec![make_provider_track("ZZZZZZ", None)];
        let score = track_title_overlap(&left, &right);
        assert!(score < 0.5, "Expected low score, got {score}");
    }

    #[test]
    fn title_overlap_empty_lists() {
        assert_eq!(track_title_overlap(&[], &[]), 0.0);
    }

    #[test]
    fn title_overlap_one_empty() {
        let left = vec![make_provider_track("Hello", None)];
        assert_eq!(track_title_overlap(&left, &[]), 0.0);
        assert_eq!(track_title_overlap(&[], &left), 0.0);
    }

    // ── best_artist_candidate ───────────────────────────────────

    #[test]
    fn best_artist_exact_match() {
        let candidates = vec![
            make_provider_artist("Radiohead"),
            make_provider_artist("Radio Company"),
        ];
        let result = best_artist_candidate("Radiohead", candidates, |_| true);
        assert!(result.is_some());
        let (artist, score) = result.unwrap();
        assert_eq!(artist.name, "Radiohead");
        assert!(score > 0.99);
    }

    #[test]
    fn best_artist_fuzzy_match() {
        let candidates = vec![
            make_provider_artist("The Black Keys"),
            make_provider_artist("Completely Unrelated Band"),
        ];
        let result = best_artist_candidate("The Black Keys", candidates, |_| true);
        assert!(result.is_some());
        let (artist, score) = result.unwrap();
        assert_eq!(artist.name, "The Black Keys");
        assert!(score > 0.95);
    }

    #[test]
    fn best_artist_below_threshold_returns_none() {
        let candidates = vec![make_provider_artist("Completely Different")];
        let result = best_artist_candidate("Radiohead", candidates, |_| true);
        assert!(result.is_none());
    }

    #[test]
    fn best_artist_predicate_filters() {
        let candidates = vec![
            make_provider_artist("Radiohead"),
            make_provider_artist("Radio Company"),
        ];
        // Predicate excludes exact match
        let result = best_artist_candidate("Radiohead", candidates, |c| c.name != "Radiohead");
        // "Radio Company" is too different from "Radiohead" for the 0.70 threshold
        // so this may or may not match depending on jaro-winkler score
        if let Some((artist, _)) = &result {
            assert_ne!(artist.name, "Radiohead");
        }
    }

    #[test]
    fn best_artist_empty_candidates() {
        let result = best_artist_candidate("Radiohead", vec![], |_| true);
        assert!(result.is_none());
    }
}
