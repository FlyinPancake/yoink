use std::{collections::HashSet, convert::Infallible};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};
use tracing::debug;
use uuid::Uuid;

use crate::{
    db,
    models::{SearchQuery, SearchResultAlbum, SearchResultArtist, SearchResultTrack, TrackInfo},
    state::AppState,
    ui::{artist_image_url, artist_profile_url},
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/library/artists", get(list_monitored_artists))
        .route("/api/library/albums", get(list_monitored_albums))
        .route("/api/downloads", get(list_download_jobs))
        .route("/api/tidal/instances", get(list_tidal_instances))
        .route("/api/albums/{album_id}/tracks", get(album_tracks))
        .route("/api/search", get(api_search))
        .route("/api/search/albums", get(api_search_albums))
        .route("/api/search/tracks", get(api_search_tracks))
        .route("/api/events", get(sse_events))
}

async fn list_monitored_artists(State(state): State<AppState>) -> impl IntoResponse {
    let artists = state.monitored_artists.read().await.clone();
    Json(artists)
}

async fn list_monitored_albums(State(state): State<AppState>) -> impl IntoResponse {
    let albums = state.monitored_albums.read().await.clone();
    Json(albums)
}

async fn list_download_jobs(State(state): State<AppState>) -> impl IntoResponse {
    let jobs = state.download_jobs.read().await.clone();
    Json(jobs)
}

async fn list_tidal_instances(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(tidal) = state.registry.tidal_provider() {
        let payload = tidal.list_instances_payload().await;
        return Json(serde_json::to_value(payload).unwrap_or_default()).into_response();
    }
    Json(serde_json::json!({"error": "Tidal provider not available"})).into_response()
}

async fn album_tracks(
    State(state): State<AppState>,
    Path(album_id): Path<Uuid>,
) -> impl IntoResponse {
    match db::load_tracks_for_album(&state.db, album_id).await {
        Ok(tracks) if !tracks.is_empty() => {
            return (StatusCode::OK, Json(tracks)).into_response();
        }
        _ => {}
    }

    let links = match db::load_album_provider_links(&state.db, album_id).await {
        Ok(links) => links,
        Err(err) => {
            debug!(album_id = %album_id, error = %err, "Failed to load album provider links");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": err.to_string()})),
            )
                .into_response();
        }
    };

    for link in &links {
        let Some(provider) = state.registry.metadata_provider(&link.provider) else {
            continue;
        };

        match provider.fetch_tracks(&link.external_id).await {
            Ok((provider_tracks, _album_extra)) => {
                let tracks: Vec<TrackInfo> = provider_tracks
                    .into_iter()
                    .map(|track| {
                        let secs = track.duration_secs;
                        let mins = secs / 60;
                        let rem = secs % 60;
                        TrackInfo {
                            id: Uuid::now_v7(),
                            title: track.title,
                            version: track.version,
                            disc_number: track.disc_number.unwrap_or(1),
                            track_number: track.track_number,
                            duration_secs: secs,
                            duration_display: format!("{mins}:{rem:02}"),
                            isrc: track.isrc,
                            explicit: track.explicit,
                            quality_override: None,
                            track_artist: track.artists,
                            file_path: None,
                            monitored: false,
                            acquired: false,
                        }
                    })
                    .collect();
                return (StatusCode::OK, Json(tracks)).into_response();
            }
            Err(err) => {
                debug!(
                    album_id = %album_id,
                    provider = %link.provider,
                    error = %err,
                    "Failed to fetch tracks from provider"
                );
            }
        }
    }

    (StatusCode::OK, Json(Vec::<TrackInfo>::new())).into_response()
}

async fn api_search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let q = match query.q.filter(|value| !value.trim().is_empty()) {
        Some(q) => q,
        None => return (StatusCode::OK, Json(Vec::<SearchResultArtist>::new())).into_response(),
    };

    let monitored = state.monitored_artists.read().await;
    let monitored_names: HashSet<String> = monitored
        .iter()
        .map(|artist| artist.name.to_ascii_lowercase())
        .collect();
    drop(monitored);

    let all_results = state.registry.search_artists_all(&q).await;
    let mut results = Vec::new();

    for (provider_id, artists) in all_results {
        for artist in &artists {
            results.push(SearchResultArtist {
                provider: provider_id.clone(),
                external_id: artist.external_id.clone(),
                name: artist.name.clone(),
                image_url: artist_image_url(&provider_id, artist, 160),
                url: artist_profile_url(artist),
                already_monitored: monitored_names.contains(&artist.name.to_ascii_lowercase()),
                disambiguation: artist.disambiguation.clone(),
                artist_type: artist.artist_type.clone(),
                country: artist.country.clone(),
                tags: artist.tags.clone(),
                popularity: artist.popularity,
            });
        }
    }

    (StatusCode::OK, Json(results)).into_response()
}

async fn api_search_albums(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let q = match query.q.filter(|value| !value.trim().is_empty()) {
        Some(q) => q,
        None => return (StatusCode::OK, Json(Vec::<SearchResultAlbum>::new())).into_response(),
    };

    let all_results = state.registry.search_albums_all(&q).await;
    let mut results = Vec::new();

    for (provider_id, albums) in all_results {
        for album in albums {
            let cover_url = album
                .cover_ref
                .as_deref()
                .map(|cover| yoink_shared::provider_image_url(&provider_id, cover, 320));

            results.push(SearchResultAlbum {
                provider: provider_id.clone(),
                external_id: album.external_id,
                title: album.title,
                album_type: album.album_type,
                release_date: album.release_date,
                cover_url,
                url: album.url,
                explicit: album.explicit,
                artist_name: album.artist_name,
                artist_external_id: album.artist_external_id,
            });
        }
    }

    (StatusCode::OK, Json(results)).into_response()
}

async fn api_search_tracks(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let q = match query.q.filter(|value| !value.trim().is_empty()) {
        Some(q) => q,
        None => return (StatusCode::OK, Json(Vec::<SearchResultTrack>::new())).into_response(),
    };

    let all_results = state.registry.search_tracks_all(&q).await;
    let mut results = Vec::new();

    for (provider_id, tracks) in all_results {
        for track in tracks {
            let secs = track.duration_secs;
            let mins = secs / 60;
            let rem = secs % 60;

            let album_cover_url = track
                .album_cover_ref
                .as_deref()
                .map(|cover| yoink_shared::provider_image_url(&provider_id, cover, 160));

            results.push(SearchResultTrack {
                provider: provider_id.clone(),
                external_id: track.external_id,
                title: track.title,
                version: track.version,
                duration_secs: track.duration_secs,
                duration_display: format!("{mins}:{rem:02}"),
                isrc: track.isrc,
                explicit: track.explicit,
                artist_name: track.artist_name,
                artist_external_id: track.artist_external_id,
                album_title: track.album_title,
                album_external_id: track.album_external_id,
                album_cover_url,
            });
        }
    }

    (StatusCode::OK, Json(results)).into_response()
}

async fn sse_events(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.sse_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(()) => Some(Ok(Event::default().event("update").data("refresh"))),
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
