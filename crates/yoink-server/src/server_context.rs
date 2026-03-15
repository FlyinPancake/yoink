use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::{actions::dispatch_action_impl, db, services, state::AppState};

/// Compute a relevance score (0.0–1.0) for a search result name against the query.
///
/// Uses a combination of:
/// - Jaro-Winkler similarity (good for short strings, prefix-biased)
/// - Exact / prefix / contains bonuses
///
/// Both `query` and `name` should be pre-lowercased.
fn search_relevance_score(query: &str, name: &str) -> f64 {
    if query == name {
        return 1.0;
    }

    let jw = strsim::jaro_winkler(query, name);

    let prefix_bonus = if name.starts_with(query) { 0.15 } else { 0.0 };

    let contains_bonus = if !name.starts_with(query) && name.contains(query) {
        0.05
    } else {
        0.0
    };

    (jw + prefix_bonus + contains_bonus).min(1.0)
}

/// Build the `ServerContext` that wires shared service functions to the real `AppState`.
pub(crate) fn build_server_context(state: &AppState) -> yoink_shared::ServerContext {
    let search_fn = build_search_fn(state);
    let search_scoped_fn = build_search_scoped_fn(state);
    let list_providers_fn = build_list_providers_fn(state);
    let fetch_tracks_fn = build_fetch_tracks_fn(state);
    let fetch_artist_links_fn = build_fetch_artist_links_fn(state);
    let fetch_album_links_fn = build_fetch_album_links_fn(state);
    let fetch_artist_match_suggestions_fn = build_fetch_artist_match_suggestions_fn(state);
    let fetch_album_match_suggestions_fn = build_fetch_album_match_suggestions_fn(state);
    let dispatch_action_fn = build_dispatch_action_fn(state);
    let preview_import_fn = build_preview_import_fn(state);
    let confirm_import_fn = build_confirm_import_fn(state);
    let fetch_artist_images_fn = build_fetch_artist_images_fn(state);
    let search_albums_fn = build_search_albums_fn(state);
    let search_tracks_fn = build_search_tracks_fn(state);
    let fetch_library_tracks_fn = build_fetch_library_tracks_fn(state);
    let browse_path_fn = build_browse_path_fn();
    let preview_external_import_fn = build_preview_external_import_fn(state);
    let confirm_external_import_fn = build_confirm_external_import_fn(state);

    yoink_shared::ServerContext {
        auth_enabled: state.auth.enabled(),
        monitored_artists: state.monitored_artists.clone(),
        monitored_albums: state.monitored_albums.clone(),
        download_jobs: state.download_jobs.clone(),
        default_quality: state.default_quality,
        search_artists: search_fn,
        search_artists_scoped: search_scoped_fn,
        list_providers: list_providers_fn,
        fetch_tracks: fetch_tracks_fn,
        fetch_artist_links: fetch_artist_links_fn,
        fetch_album_links: fetch_album_links_fn,
        fetch_artist_match_suggestions: fetch_artist_match_suggestions_fn,
        fetch_album_match_suggestions: fetch_album_match_suggestions_fn,
        dispatch_action: dispatch_action_fn,
        preview_import: preview_import_fn,
        confirm_import: confirm_import_fn,
        fetch_artist_images: fetch_artist_images_fn,
        search_albums: search_albums_fn,
        search_tracks: search_tracks_fn,
        fetch_library_tracks: fetch_library_tracks_fn,
        browse_path: browse_path_fn,
        preview_external_import: preview_external_import_fn,
        confirm_external_import: confirm_external_import_fn,
    }
}

// ── Individual callback builders ────────────────────────────────────

fn build_search_fn(state: &AppState) -> yoink_shared::SearchArtistsFn {
    let s = state.clone();
    std::sync::Arc::new(move |query: String| {
        let s = s.clone();
        Box::pin(async move {
            let all_results = s.registry.search_artists_all(&query).await;
            let query_lower = query.to_lowercase();

            let mut scored: Vec<(f64, yoink_shared::SearchArtistResult)> = Vec::new();
            for (provider_id, artists) in all_results {
                for a in artists {
                    let image_url = a
                        .image_ref
                        .as_deref()
                        .map(|r| yoink_shared::provider_image_url(&provider_id, r, 160));
                    let name_lower = a.name.to_lowercase();
                    let score = search_relevance_score(&query_lower, &name_lower);
                    scored.push((
                        score,
                        yoink_shared::SearchArtistResult {
                            provider: provider_id.clone(),
                            external_id: a.external_id,
                            name: a.name,
                            image_url,
                            url: a.url,
                            disambiguation: a.disambiguation,
                            artist_type: a.artist_type,
                            country: a.country,
                            tags: a.tags,
                            popularity: a.popularity,
                            already_monitored: None,
                        },
                    ));
                }
            }

            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            let results = scored.into_iter().map(|(_, r)| r).collect();
            Ok(results)
        })
    })
}

fn build_search_scoped_fn(state: &AppState) -> yoink_shared::SearchArtistsScopedFn {
    let s = state.clone();
    std::sync::Arc::new(move |provider_id: String, query: String| {
        let s = s.clone();
        Box::pin(async move {
            let artists = s
                .registry
                .search_artists(&provider_id, &query)
                .await
                .map_err(|e| e.to_string())?;
            let results = artists
                .into_iter()
                .map(|a| {
                    let image_url = a
                        .image_ref
                        .as_deref()
                        .map(|r| yoink_shared::provider_image_url(&provider_id, r, 160));
                    yoink_shared::SearchArtistResult {
                        provider: provider_id.clone(),
                        external_id: a.external_id,
                        name: a.name,
                        image_url,
                        url: a.url,
                        disambiguation: a.disambiguation,
                        artist_type: a.artist_type,
                        country: a.country,
                        tags: a.tags,
                        popularity: a.popularity,
                        already_monitored: None,
                    }
                })
                .collect();
            Ok(results)
        })
    })
}

fn build_list_providers_fn(state: &AppState) -> yoink_shared::ListProvidersFn {
    let registry = state.registry.clone();
    std::sync::Arc::new(move || registry.metadata_provider_ids())
}

fn build_fetch_tracks_fn(state: &AppState) -> yoink_shared::FetchTracksFn {
    let s = state.clone();
    std::sync::Arc::new(move |album_id: Uuid| {
        let s = s.clone();
        Box::pin(async move {
            load_or_backfill_album_tracks(&s, album_id)
                .await
                .map_err(Into::into)
        })
    })
}

fn build_fetch_artist_links_fn(state: &AppState) -> yoink_shared::FetchArtistLinksFn {
    let s = state.clone();
    std::sync::Arc::new(move |artist_id: Uuid| {
        let s = s.clone();
        Box::pin(async move {
            let links = db::load_artist_provider_links(&s.db, artist_id)
                .await
                .map_err(|e| format!("Failed to load provider links: {e}"))?;
            Ok(links
                .into_iter()
                .map(|l| yoink_shared::ProviderLink {
                    provider: l.provider,
                    external_id: l.external_id,
                    external_url: l.external_url,
                    external_name: l.external_name,
                })
                .collect())
        })
    })
}

fn build_fetch_album_links_fn(state: &AppState) -> yoink_shared::FetchAlbumLinksFn {
    let s = state.clone();
    std::sync::Arc::new(move |album_id: Uuid| {
        let s = s.clone();
        Box::pin(async move {
            let links = db::load_album_provider_links(&s.db, album_id)
                .await
                .map_err(|e| format!("Failed to load album provider links: {e}"))?;
            Ok(links
                .into_iter()
                .map(|l| yoink_shared::ProviderLink {
                    provider: l.provider,
                    external_id: l.external_id,
                    external_url: l.external_url,
                    external_name: l.external_title,
                })
                .collect())
        })
    })
}

fn build_fetch_artist_match_suggestions_fn(
    state: &AppState,
) -> yoink_shared::FetchArtistMatchSuggestionsFn {
    let s = state.clone();
    std::sync::Arc::new(move |artist_id: Uuid| {
        let s = s.clone();
        Box::pin(async move {
            let artist_links = db::load_artist_provider_links(&s.db, artist_id)
                .await
                .map_err(|e| format!("Failed to load artist provider links: {e}"))?;
            let linked_pairs: HashSet<(String, String)> = artist_links
                .iter()
                .map(|l| (l.provider.clone(), l.external_id.clone()))
                .collect();
            let link_lookup: HashMap<(String, String), db::ArtistProviderLink> = artist_links
                .into_iter()
                .map(|l| ((l.provider.clone(), l.external_id.clone()), l))
                .collect();

            let suggestions = db::load_match_suggestions_for_scope(&s.db, "artist", artist_id)
                .await
                .map_err(|e| format!("Failed to load artist match suggestions: {e}"))?;
            Ok(suggestions
                .into_iter()
                .map(|m| map_artist_suggestion(m, &linked_pairs, &link_lookup))
                .collect())
        })
    })
}

fn build_fetch_album_match_suggestions_fn(
    state: &AppState,
) -> yoink_shared::FetchAlbumMatchSuggestionsFn {
    let s = state.clone();
    std::sync::Arc::new(move |album_id: Uuid| {
        let s = s.clone();
        Box::pin(async move {
            let album_links = db::load_album_provider_links(&s.db, album_id)
                .await
                .map_err(|e| format!("Failed to load album provider links: {e}"))?;
            let linked_pairs: HashSet<(String, String)> = album_links
                .iter()
                .map(|l| (l.provider.clone(), l.external_id.clone()))
                .collect();
            let link_lookup: HashMap<(String, String), db::AlbumProviderLink> = album_links
                .into_iter()
                .map(|l| ((l.provider.clone(), l.external_id.clone()), l))
                .collect();

            let suggestions = db::load_match_suggestions_for_scope(&s.db, "album", album_id)
                .await
                .map_err(|e| format!("Failed to load album match suggestions: {e}"))?;
            Ok(suggestions
                .into_iter()
                .map(|m| map_album_suggestion(m, &linked_pairs, &link_lookup))
                .collect())
        })
    })
}

fn build_dispatch_action_fn(state: &AppState) -> yoink_shared::DispatchActionFn {
    let s = state.clone();
    std::sync::Arc::new(move |action: yoink_shared::ServerAction| {
        let s = s.clone();
        Box::pin(async move { dispatch_action_impl(s, action).await.map_err(Into::into) })
    })
}

fn build_preview_import_fn(state: &AppState) -> yoink_shared::PreviewImportFn {
    let s = state.clone();
    std::sync::Arc::new(move || {
        let s = s.clone();
        Box::pin(async move {
            services::preview_import_library(&s)
                .await
                .map_err(Into::into)
        })
    })
}

fn build_confirm_import_fn(state: &AppState) -> yoink_shared::ConfirmImportFn {
    let s = state.clone();
    std::sync::Arc::new(move |items: Vec<yoink_shared::ImportConfirmation>| {
        let s = s.clone();
        Box::pin(async move {
            services::confirm_import_library(&s, items)
                .await
                .map_err(Into::into)
        })
    })
}

fn build_fetch_artist_images_fn(state: &AppState) -> yoink_shared::FetchArtistImagesFn {
    let s = state.clone();
    std::sync::Arc::new(move |artist_id: Uuid| {
        let s = s.clone();
        Box::pin(async move {
            // Look up the artist name to use as a search hint for providers
            // that need to search by name (e.g. Tidal).
            let artist_name = {
                let artists = s.monitored_artists.read().await;
                artists
                    .iter()
                    .find(|a| a.id == artist_id)
                    .map(|a| a.name.clone())
            };

            let links = db::load_artist_provider_links(&s.db, artist_id)
                .await
                .map_err(|e| format!("Failed to load provider links: {e}"))?;

            tracing::debug!(
                %artist_id,
                link_count = links.len(),
                "Fetching artist images from provider links"
            );

            let mut images = Vec::new();

            for link in &links {
                let provider_id = &link.provider;
                let external_id = &link.external_id;
                let stored_ref = link.image_ref.as_deref();

                tracing::debug!(
                    %artist_id,
                    provider = %provider_id,
                    %external_id,
                    stored_image_ref = ?stored_ref,
                    "Processing provider link for artist image"
                );

                // Skip providers that don't have artist images (empty string sentinel).
                if stored_ref == Some("") {
                    tracing::debug!(
                        %artist_id,
                        provider = %provider_id,
                        "Skipping provider — empty image_ref sentinel (no artist images)"
                    );
                    continue;
                }

                let Some(provider) = s.registry.metadata_provider(provider_id) else {
                    tracing::debug!(
                        %artist_id,
                        provider = %provider_id,
                        "Skipping provider — not registered as metadata provider"
                    );
                    continue;
                };

                // Always try a fresh fetch first — stored refs can go stale
                // (e.g. Tidal rotates CDN images).
                tracing::debug!(
                    %artist_id,
                    provider = %provider_id,
                    %external_id,
                    "Attempting fresh image ref fetch from provider"
                );
                if let Some(image_ref) = provider
                    .fetch_artist_image_ref(external_id, artist_name.as_deref())
                    .await
                {
                    let url = yoink_shared::provider_image_url(provider_id, &image_ref, 640);
                    tracing::debug!(
                        %artist_id,
                        provider = %provider_id,
                        %image_ref,
                        %url,
                        "Got fresh image ref from provider"
                    );
                    images.push(yoink_shared::ArtistImageOption {
                        provider: provider_id.clone(),
                        image_url: url,
                    });

                    // Update the stored ref so other code paths use the fresh one
                    let mut updated_link = link.clone();
                    updated_link.image_ref = Some(image_ref);
                    if let Err(e) = db::upsert_artist_provider_link(&s.db, &updated_link).await {
                        tracing::warn!(error = %e, "Failed to persist updated artist provider link image_ref");
                    }
                    continue;
                }

                tracing::debug!(
                    %artist_id,
                    provider = %provider_id,
                    "Provider returned no fresh image ref, falling back to stored ref"
                );

                // Fall back to the stored image_ref if the provider doesn't
                // implement fetch_artist_image_ref.
                if let Some(ref stored_ref) = link.image_ref {
                    // Extract the raw image ref from the stored value
                    let raw_ref = if stored_ref.starts_with("/api/image/") {
                        // Stored as proxy URL "/api/image/{provider}/{ref}/{size}"
                        // Extract the raw ref (the segment between provider and size)
                        let extracted = stored_ref
                            .strip_prefix(&format!("/api/image/{provider_id}/"))
                            .and_then(|rest| rest.rsplit_once('/').map(|(r, _)| r.to_string()));
                        tracing::debug!(
                            %artist_id,
                            provider = %provider_id,
                            %stored_ref,
                            extracted_raw_ref = ?extracted,
                            "Extracted raw ref from stored proxy URL"
                        );
                        extracted
                    } else {
                        tracing::debug!(
                            %artist_id,
                            provider = %provider_id,
                            %stored_ref,
                            "Using stored ref directly (not a proxy URL)"
                        );
                        Some(stored_ref.clone())
                    };

                    if let Some(raw_ref) = raw_ref {
                        let url = yoink_shared::provider_image_url(provider_id, &raw_ref, 640);
                        tracing::debug!(
                            %artist_id,
                            provider = %provider_id,
                            %raw_ref,
                            %url,
                            "Adding stored image option"
                        );
                        images.push(yoink_shared::ArtistImageOption {
                            provider: provider_id.clone(),
                            image_url: url,
                        });
                    } else {
                        tracing::warn!(
                            %artist_id,
                            provider = %provider_id,
                            %stored_ref,
                            "Failed to extract raw image ref from stored value"
                        );
                    }
                } else {
                    tracing::debug!(
                        %artist_id,
                        provider = %provider_id,
                        "No stored image_ref and no fresh ref — skipping provider"
                    );
                }
            }

            tracing::debug!(
                %artist_id,
                image_count = images.len(),
                providers = ?images.iter().map(|i| i.provider.as_str()).collect::<Vec<_>>(),
                "Finished fetching artist images"
            );

            Ok(images)
        })
    })
}

fn build_search_albums_fn(state: &AppState) -> yoink_shared::SearchAlbumsFn {
    let s = state.clone();
    std::sync::Arc::new(move |query: String| {
        let s = s.clone();
        Box::pin(async move {
            let all_results = s.registry.search_albums_all(&query).await;
            let mut results = Vec::new();

            for (provider_id, albums) in all_results {
                for a in albums {
                    let cover_url = a
                        .cover_ref
                        .as_deref()
                        .map(|c| yoink_shared::provider_image_url(&provider_id, c, 320));

                    results.push(yoink_shared::SearchAlbumResult {
                        provider: provider_id.clone(),
                        external_id: a.external_id,
                        title: a.title,
                        album_type: a.album_type,
                        release_date: a.release_date,
                        cover_url,
                        url: a.url,
                        explicit: a.explicit,
                        artist_name: a.artist_name,
                        artist_external_id: a.artist_external_id,
                        already_added: None,
                    });
                }
            }

            Ok(results)
        })
    })
}

fn build_search_tracks_fn(state: &AppState) -> yoink_shared::SearchTracksFn {
    let s = state.clone();
    std::sync::Arc::new(move |query: String| {
        let s = s.clone();
        Box::pin(async move {
            let all_results = s.registry.search_tracks_all(&query).await;
            let mut results = Vec::new();

            for (provider_id, tracks) in all_results {
                for t in tracks {
                    let album_cover_url = t
                        .album_cover_ref
                        .as_deref()
                        .map(|c| yoink_shared::provider_image_url(&provider_id, c, 160));

                    results.push(yoink_shared::SearchTrackResult {
                        provider: provider_id.clone(),
                        external_id: t.external_id,
                        title: t.title,
                        version: t.version,
                        duration_secs: t.duration_secs,
                        duration_display: yoink_shared::format_duration(t.duration_secs),
                        isrc: t.isrc,
                        explicit: t.explicit,
                        artist_name: t.artist_name,
                        artist_external_id: t.artist_external_id,
                        album_title: t.album_title,
                        album_external_id: t.album_external_id,
                        album_cover_url,
                        already_added: None,
                    });
                }
            }

            Ok(results)
        })
    })
}

fn build_fetch_library_tracks_fn(state: &AppState) -> yoink_shared::FetchLibraryTracksFn {
    let s = state.clone();
    std::sync::Arc::new(move || {
        let s = s.clone();
        Box::pin(async move {
            let artists = s.monitored_artists.read().await.clone();
            let artist_names: HashMap<Uuid, String> = artists
                .into_iter()
                .map(|artist| (artist.id, artist.name))
                .collect();
            let albums = s.monitored_albums.read().await.clone();

            let mut rows = Vec::new();
            for album in albums {
                let tracks = load_or_backfill_album_tracks(&s, album.id)
                    .await
                    .map_err(yoink_shared::YoinkError::from)?;
                if tracks.is_empty() {
                    continue;
                }

                let include_album = album.monitored
                    || album.acquired
                    || tracks.iter().any(|t| t.monitored || t.acquired);
                if !include_album {
                    continue;
                }

                let artist_name = artist_names
                    .get(&album.artist_id)
                    .cloned()
                    .unwrap_or_else(|| yoink_shared::UNKNOWN_ARTIST.to_string());

                rows.extend(tracks.into_iter().map(|track| yoink_shared::LibraryTrack {
                    track,
                    album_id: album.id,
                    album_title: album.title.clone(),
                    album_cover_url: album.cover_url.clone(),
                    artist_id: album.artist_id,
                    artist_name: artist_name.clone(),
                }));
            }

            rows.sort_by(|a, b| {
                a.artist_name
                    .cmp(&b.artist_name)
                    .then_with(|| a.album_title.cmp(&b.album_title))
                    .then_with(|| a.track.disc_number.cmp(&b.track.disc_number))
                    .then_with(|| a.track.track_number.cmp(&b.track.track_number))
            });

            Ok(rows)
        })
    })
}

fn build_browse_path_fn() -> yoink_shared::BrowsePathFn {
    std::sync::Arc::new(move |path: String| {
        Box::pin(async move { services::browse_path(&path).await.map_err(Into::into) })
    })
}

fn build_preview_external_import_fn(state: &AppState) -> yoink_shared::PreviewExternalImportFn {
    let s = state.clone();
    std::sync::Arc::new(move |source_path: String| {
        let s = s.clone();
        Box::pin(async move {
            services::preview_external_import(&s, &source_path)
                .await
                .map_err(Into::into)
        })
    })
}

fn build_confirm_external_import_fn(state: &AppState) -> yoink_shared::ConfirmExternalImportFn {
    let s = state.clone();
    std::sync::Arc::new(
        move |confirmation: yoink_shared::ExternalImportConfirmation| {
            let s = s.clone();
            Box::pin(async move {
                services::confirm_external_import(
                    &s,
                    &confirmation.source_path,
                    confirmation.mode,
                    confirmation.items,
                )
                .await
                .map_err(Into::into)
            })
        },
    )
}

async fn load_or_backfill_album_tracks(
    state: &AppState,
    album_id: Uuid,
) -> Result<Vec<yoink_shared::TrackInfo>, String> {
    let album = {
        let albums = state.monitored_albums.read().await;
        albums.iter().find(|album| album.id == album_id).cloned()
    };

    let tracks = db::load_tracks_for_album(&state.db, album_id)
        .await
        .map_err(|e| format!("Failed to load tracks: {e}"))?;

    // Track monitored/acquired flags are now kept in sync when the
    // album-level monitored state changes (see toggle_album_monitor /
    // bulk_monitor), so the lazy one-way promotion that used to live
    // here is no longer needed.

    let needs_artist_backfill =
        !tracks.is_empty() && tracks.iter().all(|track| track.track_artist.is_none());
    if !tracks.is_empty() && !needs_artist_backfill {
        return Ok(tracks);
    }

    let links = db::load_album_provider_links(&state.db, album_id)
        .await
        .map_err(|e| format!("Failed to load album links: {e}"))?;

    for link in &links {
        let Some(provider) = state.registry.metadata_provider(&link.provider) else {
            continue;
        };

        match provider.fetch_tracks(&link.external_id).await {
            Ok((provider_tracks, _album_extra)) => {
                for provider_track in provider_tracks {
                    let local_track_id = if let Ok(Some(track_id)) =
                        db::find_track_by_provider_link(
                            &state.db,
                            &link.provider,
                            &provider_track.external_id,
                        )
                        .await
                    {
                        track_id
                    } else if let Some(ref isrc) = provider_track.isrc {
                        if let Ok(Some(track_id)) =
                            db::find_track_by_album_isrc(&state.db, album_id, isrc).await
                        {
                            track_id
                        } else {
                            Uuid::now_v7()
                        }
                    } else {
                        db::find_track_by_album_position(
                            &state.db,
                            album_id,
                            provider_track.disc_number.unwrap_or(1),
                            provider_track.track_number,
                        )
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or_else(Uuid::now_v7)
                    };

                    let existing_track = tracks.iter().find(|track| track.id == local_track_id);
                    let merged_artist = provider_track
                        .artists
                        .clone()
                        .or_else(|| existing_track.and_then(|track| track.track_artist.clone()));
                    let provider_ext_id = provider_track.external_id.clone();
                    let track_info =
                        provider_track.into_track_info(crate::providers::LocalTrackOverrides {
                            id: local_track_id,
                            quality_override: existing_track
                                .and_then(|track| track.quality_override),
                            track_artist: merged_artist,
                            file_path: existing_track.and_then(|track| track.file_path.clone()),
                            monitored: existing_track.map(|track| track.monitored).unwrap_or_else(
                                || album.as_ref().map(|album| album.monitored).unwrap_or(false),
                            ),
                            acquired: existing_track.map(|track| track.acquired).unwrap_or_else(
                                || {
                                    album
                                        .as_ref()
                                        .map(|album| album.monitored && album.acquired)
                                        .unwrap_or(false)
                                },
                            ),
                            ..Default::default()
                        });

                    db::upsert_track(&state.db, &track_info, album_id)
                        .await
                        .map_err(|e| format!("failed to persist track: {e}"))?;
                    db::upsert_track_provider_link(
                        &state.db,
                        local_track_id,
                        &link.provider,
                        &provider_ext_id,
                    )
                    .await
                    .map_err(|e| format!("failed to persist track provider link: {e}"))?;
                }

                let mut persisted = db::load_tracks_for_album(&state.db, album_id)
                    .await
                    .map_err(|e| format!("Failed to reload tracks: {e}"))?;
                persisted.sort_by(|a, b| {
                    a.disc_number
                        .cmp(&b.disc_number)
                        .then_with(|| a.track_number.cmp(&b.track_number))
                });
                return Ok(persisted);
            }
            Err(_) => continue,
        }
    }

    Ok(tracks)
}

// ── Suggestion mapping helpers ──────────────────────────────────────

fn resolve_suggestion_side(
    m: &db::MatchSuggestion,
    linked_pairs: &HashSet<(String, String)>,
) -> bool {
    let left_linked = linked_pairs.contains(&(m.left_provider.clone(), m.left_external_id.clone()));
    let right_linked =
        linked_pairs.contains(&(m.right_provider.clone(), m.right_external_id.clone()));
    if left_linked && !right_linked {
        true // use right
    } else {
        !right_linked || left_linked
    }
}

fn map_artist_suggestion(
    m: db::MatchSuggestion,
    linked_pairs: &HashSet<(String, String)>,
    link_lookup: &HashMap<(String, String), db::ArtistProviderLink>,
) -> yoink_shared::MatchSuggestion {
    let use_right = resolve_suggestion_side(&m, linked_pairs);

    let (provider, external_id, external_name, external_url, image_ref) = if use_right {
        (
            m.right_provider.clone(),
            m.right_external_id.clone(),
            m.external_name.clone(),
            m.external_url.clone(),
            m.image_ref.clone(),
        )
    } else if let Some(link) =
        link_lookup.get(&(m.left_provider.clone(), m.left_external_id.clone()))
    {
        (
            m.left_provider.clone(),
            m.left_external_id.clone(),
            link.external_name.clone(),
            link.external_url.clone(),
            link.image_ref.clone(),
        )
    } else {
        (
            m.left_provider.clone(),
            m.left_external_id.clone(),
            None,
            None,
            None,
        )
    };

    let image_url = image_ref
        .as_deref()
        .map(|r| yoink_shared::provider_image_url(&provider, r, 160));
    yoink_shared::MatchSuggestion {
        id: m.id,
        scope_type: m.scope_type,
        scope_id: m.scope_id,
        left_provider: m.left_provider,
        left_external_id: m.left_external_id,
        right_provider: provider,
        right_external_id: external_id,
        match_kind: m.match_kind,
        confidence: m.confidence,
        explanation: m.explanation,
        external_name,
        external_url,
        image_url,
        disambiguation: if use_right { m.disambiguation } else { None },
        artist_type: if use_right { m.artist_type } else { None },
        country: if use_right { m.country } else { None },
        tags: if use_right { m.tags } else { Vec::new() },
        popularity: if use_right { m.popularity } else { None },
        status: m.status,
    }
}

fn map_album_suggestion(
    m: db::MatchSuggestion,
    linked_pairs: &HashSet<(String, String)>,
    link_lookup: &HashMap<(String, String), db::AlbumProviderLink>,
) -> yoink_shared::MatchSuggestion {
    let use_right = resolve_suggestion_side(&m, linked_pairs);

    let (provider, external_id, external_name, external_url, image_ref) = if use_right {
        (
            m.right_provider.clone(),
            m.right_external_id.clone(),
            m.external_name.clone(),
            m.external_url.clone(),
            m.image_ref.clone(),
        )
    } else if let Some(link) =
        link_lookup.get(&(m.left_provider.clone(), m.left_external_id.clone()))
    {
        (
            m.left_provider.clone(),
            m.left_external_id.clone(),
            link.external_title.clone(),
            link.external_url.clone(),
            link.cover_ref.clone(),
        )
    } else {
        (
            m.left_provider.clone(),
            m.left_external_id.clone(),
            None,
            None,
            None,
        )
    };

    let image_url = image_ref
        .as_deref()
        .map(|r| yoink_shared::provider_image_url(&provider, r, 160));
    yoink_shared::MatchSuggestion {
        id: m.id,
        scope_type: m.scope_type,
        scope_id: m.scope_id,
        left_provider: m.left_provider,
        left_external_id: m.left_external_id,
        right_provider: provider,
        right_external_id: external_id,
        match_kind: m.match_kind,
        confidence: m.confidence,
        explanation: m.explanation,
        external_name,
        external_url,
        image_url,
        disambiguation: None,
        artist_type: None,
        country: None,
        tags: Vec::new(),
        popularity: None,
        status: m.status,
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use super::*;
    use crate::{
        providers::{MetadataProvider, ProviderTrack, registry::ProviderRegistry},
        test_helpers::{
            MockMetadataProvider, seed_album, seed_album_provider_link, seed_artist, seed_tracks,
            test_app_state_with_registry,
        },
    };

    #[test]
    fn exact_match_returns_one() {
        assert_eq!(search_relevance_score("radiohead", "radiohead"), 1.0);
    }

    #[test]
    fn prefix_match_gets_bonus() {
        let prefix_score = search_relevance_score("radio", "radiohead");
        let no_prefix_score = search_relevance_score("radio", "the radiohead");
        assert!(prefix_score > no_prefix_score);
    }

    #[test]
    fn contains_match_gets_smaller_bonus() {
        let contains_score = search_relevance_score("head", "radiohead");
        // "head" is contained but not a prefix, so gets a small bonus
        // vs. something completely unrelated
        let unrelated_score = search_relevance_score("head", "metallica");
        assert!(contains_score > unrelated_score);
    }

    #[test]
    fn completely_different_low_score() {
        let score = search_relevance_score("radiohead", "metallica");
        assert!(score < 0.6, "Expected low score, got {score}");
    }

    #[test]
    fn score_capped_at_one() {
        // Even with both bonuses, score should not exceed 1.0
        let score = search_relevance_score("a", "a very long name");
        assert!(score <= 1.0);
    }

    #[test]
    fn empty_query_vs_name() {
        // Edge case: should not panic
        let _ = search_relevance_score("", "something");
        let _ = search_relevance_score("something", "");
    }

    #[tokio::test]
    async fn fetch_tracks_backfill_inherits_album_flags() {
        let mock = Arc::new(MockMetadataProvider::new("mock"));
        *mock.fetch_tracks_result.lock().await = Ok((
            vec![
                ProviderTrack {
                    external_id: "trk-1".to_string(),
                    title: "Track 1".to_string(),
                    version: None,
                    track_number: 1,
                    disc_number: Some(1),
                    duration_secs: 180,
                    isrc: Some("USRC12340001".to_string()),
                    artists: Some("Artist".to_string()),
                    explicit: false,
                    extra: HashMap::new(),
                },
                ProviderTrack {
                    external_id: "trk-2".to_string(),
                    title: "Track 2".to_string(),
                    version: None,
                    track_number: 2,
                    disc_number: Some(1),
                    duration_secs: 200,
                    isrc: Some("USRC12340002".to_string()),
                    artists: Some("Artist".to_string()),
                    explicit: false,
                    extra: HashMap::new(),
                },
            ],
            HashMap::new(),
        ));

        let mut registry = ProviderRegistry::new();
        registry.register_metadata(mock as Arc<dyn MetadataProvider>);
        let (state, _tmp) = test_app_state_with_registry(registry).await;

        let artist = seed_artist(&state.db, "Artist").await;
        let mut album = seed_album(&state.db, artist.id, "Album").await;
        album.acquired = true;
        album.wanted = false;
        db::upsert_album(&state.db, &album).await.unwrap();
        seed_album_provider_link(&state.db, album.id, "mock", "alb-1").await;

        state.monitored_artists.write().await.push(artist.clone());
        state.monitored_albums.write().await.push(album.clone());

        let ctx = build_server_context(&state);
        let tracks = (ctx.fetch_tracks)(album.id).await.unwrap();

        assert_eq!(tracks.len(), 2);
        assert!(tracks.iter().all(|track| track.monitored));
        assert!(tracks.iter().all(|track| track.acquired));
    }

    #[tokio::test]
    async fn fetch_library_tracks_backfills_missing_synced_album_tracks() {
        let mock = Arc::new(MockMetadataProvider::new("mock"));
        *mock.fetch_tracks_result.lock().await = Ok((
            vec![ProviderTrack {
                external_id: "trk-1".to_string(),
                title: "Track 1".to_string(),
                version: None,
                track_number: 1,
                disc_number: Some(1),
                duration_secs: 180,
                isrc: Some("USRC12340001".to_string()),
                artists: Some("Artist".to_string()),
                explicit: false,
                extra: HashMap::new(),
            }],
            HashMap::new(),
        ));

        let mut registry = ProviderRegistry::new();
        registry.register_metadata(mock as Arc<dyn MetadataProvider>);
        let (state, _tmp) = test_app_state_with_registry(registry).await;

        let artist = seed_artist(&state.db, "Artist").await;
        let album = seed_album(&state.db, artist.id, "Album").await;
        db::upsert_album(&state.db, &album).await.unwrap();
        seed_album_provider_link(&state.db, album.id, "mock", "alb-1").await;

        state.monitored_artists.write().await.push(artist.clone());
        state.monitored_albums.write().await.push(album.clone());

        let ctx = build_server_context(&state);
        let tracks = (ctx.fetch_library_tracks)().await.unwrap();

        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].album_id, album.id);
        assert_eq!(tracks[0].artist_id, artist.id);
        assert_eq!(tracks[0].track.title, "Track 1");
        assert!(tracks[0].track.monitored);
    }

    #[tokio::test]
    async fn fetch_tracks_partial_album_does_not_mark_all_tracks_acquired() {
        let mock = Arc::new(MockMetadataProvider::new("mock"));
        *mock.fetch_tracks_result.lock().await = Ok((
            vec![
                ProviderTrack {
                    external_id: "trk-1".to_string(),
                    title: "Track 1".to_string(),
                    version: None,
                    track_number: 1,
                    disc_number: Some(1),
                    duration_secs: 180,
                    isrc: Some("USRC12340001".to_string()),
                    artists: Some("Artist".to_string()),
                    explicit: false,
                    extra: HashMap::new(),
                },
                ProviderTrack {
                    external_id: "trk-2".to_string(),
                    title: "Track 2".to_string(),
                    version: None,
                    track_number: 2,
                    disc_number: Some(1),
                    duration_secs: 200,
                    isrc: Some("USRC12340002".to_string()),
                    artists: Some("Artist".to_string()),
                    explicit: false,
                    extra: HashMap::new(),
                },
            ],
            HashMap::new(),
        ));

        let mut registry = ProviderRegistry::new();
        registry.register_metadata(mock as Arc<dyn MetadataProvider>);
        let (state, _tmp) = test_app_state_with_registry(registry).await;

        let artist = seed_artist(&state.db, "Artist").await;
        let mut album = seed_album(&state.db, artist.id, "Album").await;
        album.monitored = false;
        album.acquired = true;
        album.wanted = false;
        album.partially_wanted = false;
        db::upsert_album(&state.db, &album).await.unwrap();
        let tracks = seed_tracks(&state.db, album.id, 2).await;
        db::update_track_flags(&state.db, tracks[0].id, true, true)
            .await
            .unwrap();
        seed_album_provider_link(&state.db, album.id, "mock", "alb-1").await;

        state.monitored_artists.write().await.push(artist);
        state.monitored_albums.write().await.push(album.clone());

        let ctx = build_server_context(&state);
        let loaded = (ctx.fetch_tracks)(album.id).await.unwrap();

        assert_eq!(loaded.len(), 2);
        assert!(loaded[0].acquired);
        assert!(loaded[0].monitored);
        assert!(!loaded[1].acquired);
        assert!(!loaded[1].monitored);
    }
}
