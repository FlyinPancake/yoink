use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use yoink_shared::{
    DownloadJob, MatchSuggestion, MonitoredAlbum, MonitoredArtist, ProviderLink, Quality,
    SearchAlbumResult, ServerAction, TrackInfo,
};

use crate::{
    actions::dispatch_action_impl, db, error::AppError, server_context::build_server_context,
    state::AppState,
};

use super::helpers::{
    ApiErrorResponse, app_error_response, enrich_album_results, parse_uuid as parse_uuid_param,
    yoink_error_response,
};

pub(crate) const TAG: &str = "Album";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for album search, lookup, and lifecycle";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;
type ApiStatusResult = Result<StatusCode, ApiErrorResponse>;

#[derive(Debug, Deserialize, ToSchema)]
struct AlbumSearchQuery {
    query: String,
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateAlbumRequest {
    provider: String,
    external_album_id: String,
    artist_external_id: String,
    artist_name: String,
    monitor_all: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
struct ToggleAlbumMonitorRequest {
    monitored: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
struct SetAlbumQualityRequest {
    #[serde(default)]
    quality: Option<Quality>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct RemoveAlbumFilesQuery {
    #[serde(default)]
    unmonitor: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
struct ToggleTrackMonitorRequest {
    monitored: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
struct SetTrackQualityRequest {
    #[serde(default)]
    quality: Option<Quality>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct AlbumArtistRequest {
    artist_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
struct MergeAlbumsRequest {
    target_album_id: Uuid,
    source_album_id: Uuid,
    #[serde(default)]
    result_title: Option<String>,
    #[serde(default)]
    result_cover_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
struct ResolvedArtistCredit {
    name: String,
    artist_id: Option<Uuid>,
    provider: Option<String>,
    external_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
struct AlbumDetailResponse {
    album: MonitoredAlbum,
    artist: Option<MonitoredArtist>,
    album_artists: Vec<ResolvedArtistCredit>,
    tracks: Vec<TrackInfo>,
    jobs: Vec<DownloadJob>,
    provider_links: Vec<ProviderLink>,
    match_suggestions: Vec<MatchSuggestion>,
    default_quality: Quality,
}

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(search_albums))
        .routes(routes!(list_albums))
        .routes(routes!(create_album))
        .routes(routes!(merge_albums))
        .routes(routes!(get_album))
        .routes(routes!(toggle_album_monitor))
        .routes(routes!(set_album_quality))
        .routes(routes!(remove_album_files))
        .routes(routes!(retry_download))
        .routes(routes!(list_album_providers))
        .routes(routes!(list_album_tracks))
        .routes(routes!(bulk_toggle_track_monitor))
        .routes(routes!(toggle_track_monitor))
        .routes(routes!(set_track_quality))
        .routes(routes!(add_album_artist))
        .routes(routes!(remove_album_artist))
}

/// Search Albums
///
/// Searches all registered metadata providers for albums matching the query
/// string and returns the aggregated results.
#[utoipa::path(
    get,
    path = "/search",
    tag = TAG,
    params(
        ("query" = String, Query, description = "Album search query")
    ),
    responses(
        (status = 200, description = "Search results across all providers", body = Vec<SearchAlbumResult>),
        (status = 503, description = "Provider search unavailable"),
    )
)]
async fn search_albums(
    State(state): State<AppState>,
    Query(query): Query<AlbumSearchQuery>,
) -> ApiResult<Vec<SearchAlbumResult>> {
    let trimmed = query.query.trim();
    if trimmed.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let ctx = build_server_context(&state);
    let mut results = (ctx.search_albums)(trimmed.to_string())
        .await
        .map_err(yoink_error_response)?;
    enrich_album_results(&state.db, &mut results).await;
    Ok(Json(results))
}

/// List Albums
///
/// Returns all locally stored albums from the library database.
#[utoipa::path(
    get,
    path = "/",
    tag = TAG,
    responses(
        (status = 200, description = "All local albums", body = Vec<MonitoredAlbum>),
        (status = 500, description = "Failed to load albums"),
    )
)]
async fn list_albums(State(state): State<AppState>) -> ApiResult<Vec<MonitoredAlbum>> {
    let albums = db::load_albums(&state.db)
        .await
        .map_err(|error| app_error_response(error.into()))?;
    Ok(Json(albums))
}

/// Create Album
///
/// Adds an album from provider metadata, persists the provider link, stores
/// tracks, and optionally marks the album fully monitored.
#[utoipa::path(
    post,
    path = "/",
    tag = TAG,
    request_body = CreateAlbumRequest,
    responses(
        (status = 201, description = "Album created"),
        (status = 404, description = "Provider album not found"),
        (status = 500, description = "Failed to create album"),
    )
)]
async fn create_album(
    State(state): State<AppState>,
    Json(request): Json<CreateAlbumRequest>,
) -> ApiStatusResult {
    dispatch_action_impl(
        state,
        ServerAction::AddAlbum {
            provider: request.provider,
            external_album_id: request.external_album_id,
            artist_external_id: request.artist_external_id,
            artist_name: request.artist_name,
            monitor_all: request.monitor_all,
        },
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::CREATED)
}

/// Merge Albums
///
/// Merges a source album into a target album and optionally overrides the
/// surviving title and cover image.
#[utoipa::path(
    post,
    path = "/merge",
    tag = TAG,
    request_body = MergeAlbumsRequest,
    responses(
        (status = 204, description = "Albums merged"),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to merge albums"),
    )
)]
async fn merge_albums(
    State(state): State<AppState>,
    Json(request): Json<MergeAlbumsRequest>,
) -> ApiStatusResult {
    require_album(&state, request.target_album_id).await?;
    require_album(&state, request.source_album_id).await?;

    dispatch_action_impl(
        state,
        ServerAction::MergeAlbums {
            target_album_id: request.target_album_id,
            source_album_id: request.source_album_id,
            result_title: request.result_title,
            result_cover_url: request.result_cover_url,
        },
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get Album
///
/// Returns the local album, resolved artists, tracks, provider links, download
/// jobs, and match suggestions for a single album.
#[utoipa::path(
    get,
    path = "/{album_id}",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID")
    ),
    responses(
        (status = 200, description = "Album detail payload", body = AlbumDetailResponse),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to load album detail"),
    )
)]
async fn get_album(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
) -> ApiResult<AlbumDetailResponse> {
    let album_id = parse_album_id(&album_id)?;
    let album = require_album(&state, album_id).await?;

    let (artist, album_artists) = {
        let artists = state.monitored_artists.read().await;
        let primary_artist = artists
            .iter()
            .find(|artist| artist.id == album.artist_id)
            .cloned();

        let album_artists = if !album.artist_credits.is_empty() {
            album
                .artist_credits
                .iter()
                .map(|credit| {
                    let local_id = album
                        .artist_ids
                        .iter()
                        .find(|&&artist_id| {
                            artists
                                .iter()
                                .any(|artist| artist.id == artist_id && artist.name == credit.name)
                        })
                        .copied();
                    ResolvedArtistCredit {
                        name: credit.name.clone(),
                        artist_id: local_id,
                        provider: credit.provider.clone(),
                        external_id: credit.external_id.clone(),
                    }
                })
                .collect()
        } else {
            album
                .artist_ids
                .iter()
                .filter_map(|artist_id| {
                    artists
                        .iter()
                        .find(|artist| artist.id == *artist_id)
                        .map(|artist| ResolvedArtistCredit {
                            name: artist.name.clone(),
                            artist_id: Some(artist.id),
                            provider: None,
                            external_id: None,
                        })
                })
                .collect()
        };

        (primary_artist, album_artists)
    };

    let ctx = build_server_context(&state);
    let tracks_fut = (ctx.fetch_tracks)(album_id);
    let links_fut = (ctx.fetch_album_links)(album_id);
    let suggestions_fut = (ctx.fetch_album_match_suggestions)(album_id);
    let (tracks, provider_links, match_suggestions) =
        tokio::join!(tracks_fut, links_fut, suggestions_fut);

    Ok(Json(AlbumDetailResponse {
        album,
        artist,
        album_artists,
        tracks: tracks.map_err(yoink_error_response)?,
        jobs: state.download_jobs.read().await.clone(),
        provider_links: provider_links.map_err(yoink_error_response)?,
        match_suggestions: match_suggestions.map_err(yoink_error_response)?,
        default_quality: state.default_quality,
    }))
}

/// Toggle Album Monitor
///
/// Enables or disables album-level monitoring for a local album.
#[utoipa::path(
    patch,
    path = "/{album_id}/monitor",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID")
    ),
    request_body = ToggleAlbumMonitorRequest,
    responses(
        (status = 204, description = "Album monitor flag updated"),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to toggle album monitor"),
    )
)]
async fn toggle_album_monitor(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
    Json(request): Json<ToggleAlbumMonitorRequest>,
) -> ApiStatusResult {
    let album_id = parse_album_id(&album_id)?;
    require_album(&state, album_id).await?;

    dispatch_action_impl(
        state,
        ServerAction::ToggleAlbumMonitor {
            album_id,
            monitored: request.monitored,
        },
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Set Album Quality
///
/// Sets or clears the album-level quality override for a local album.
#[utoipa::path(
    patch,
    path = "/{album_id}/quality",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID")
    ),
    request_body = SetAlbumQualityRequest,
    responses(
        (status = 204, description = "Album quality updated"),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to update album quality"),
    )
)]
async fn set_album_quality(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
    Json(request): Json<SetAlbumQualityRequest>,
) -> ApiStatusResult {
    let album_id = parse_album_id(&album_id)?;
    require_album(&state, album_id).await?;

    dispatch_action_impl(
        state,
        ServerAction::SetAlbumQuality {
            album_id,
            quality: request.quality,
        },
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Remove Album Files
///
/// Removes downloaded files for a local album and optionally unmonitors the
/// album afterwards.
#[utoipa::path(
    delete,
    path = "/{album_id}/file",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID"),
        ("unmonitor" = Option<bool>, Query, description = "Whether to unmonitor the album after file removal")
    ),
    responses(
        (status = 204, description = "Album files removed"),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to remove album files"),
    )
)]
async fn remove_album_files(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
    Query(query): Query<RemoveAlbumFilesQuery>,
) -> ApiStatusResult {
    let album_id = parse_album_id(&album_id)?;
    require_album(&state, album_id).await?;

    dispatch_action_impl(
        state,
        ServerAction::RemoveAlbumFiles {
            album_id,
            unmonitor: query.unmonitor,
        },
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Retry Download
///
/// Requeues a failed album download or creates a new queued job if the album is
/// still wanted.
#[utoipa::path(
    post,
    path = "/{album_id}/download/retry",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID")
    ),
    responses(
        (status = 204, description = "Album download queued"),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to retry album download"),
    )
)]
async fn retry_download(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
) -> ApiStatusResult {
    let album_id = parse_album_id(&album_id)?;
    require_album(&state, album_id).await?;

    dispatch_action_impl(state, ServerAction::RetryDownload { album_id })
        .await
        .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

/// List Album Providers
///
/// Returns the provider links currently attached to a local album.
#[utoipa::path(
    get,
    path = "/{album_id}/provider",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID")
    ),
    responses(
        (status = 200, description = "Provider links for the album", body = Vec<ProviderLink>),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to load album providers"),
    )
)]
async fn list_album_providers(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
) -> ApiResult<Vec<ProviderLink>> {
    let album_id = parse_album_id(&album_id)?;
    require_album(&state, album_id).await?;

    let ctx = build_server_context(&state);
    let links = (ctx.fetch_album_links)(album_id)
        .await
        .map_err(yoink_error_response)?;
    Ok(Json(links))
}

/// List Album Tracks
///
/// Returns all tracks currently associated with a local album.
#[utoipa::path(
    get,
    path = "/{album_id}/track",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID")
    ),
    responses(
        (status = 200, description = "Tracks for the album", body = Vec<TrackInfo>),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to load album tracks"),
    )
)]
async fn list_album_tracks(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
) -> ApiResult<Vec<TrackInfo>> {
    let album_id = parse_album_id(&album_id)?;
    require_album(&state, album_id).await?;

    let ctx = build_server_context(&state);
    let tracks = (ctx.fetch_tracks)(album_id)
        .await
        .map_err(yoink_error_response)?;
    Ok(Json(tracks))
}

/// Bulk Toggle Track Monitor
///
/// Enables or disables monitoring for every track on a local album.
#[utoipa::path(
    patch,
    path = "/{album_id}/track/monitor",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID")
    ),
    request_body = ToggleTrackMonitorRequest,
    responses(
        (status = 204, description = "Track monitor flags updated"),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to update track monitoring"),
    )
)]
async fn bulk_toggle_track_monitor(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
    Json(request): Json<ToggleTrackMonitorRequest>,
) -> ApiStatusResult {
    let album_id = parse_album_id(&album_id)?;
    require_album(&state, album_id).await?;

    dispatch_action_impl(
        state,
        ServerAction::BulkToggleTrackMonitor {
            album_id,
            monitored: request.monitored,
        },
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Toggle Track Monitor
///
/// Enables or disables monitoring for a single track belonging to a local
/// album.
#[utoipa::path(
    patch,
    path = "/{album_id}/track/{track_id}/monitor",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID"),
        ("track_id" = String, Path, description = "Track UUID")
    ),
    request_body = ToggleTrackMonitorRequest,
    responses(
        (status = 204, description = "Track monitor flag updated"),
        (status = 404, description = "Track not found"),
        (status = 500, description = "Failed to toggle track monitoring"),
    )
)]
async fn toggle_track_monitor(
    State(state): State<AppState>,
    Path((album_id, track_id)): Path<(String, String)>,
    Json(request): Json<ToggleTrackMonitorRequest>,
) -> ApiStatusResult {
    let album_id = parse_album_id(&album_id)?;
    let track_id = parse_track_id(&track_id)?;
    require_album(&state, album_id).await?;
    require_track(&state, album_id, track_id).await?;

    dispatch_action_impl(
        state,
        ServerAction::ToggleTrackMonitor {
            track_id,
            album_id,
            monitored: request.monitored,
        },
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Set Track Quality
///
/// Sets or clears the quality override for a single track on a local album.
#[utoipa::path(
    patch,
    path = "/{album_id}/track/{track_id}/quality",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID"),
        ("track_id" = String, Path, description = "Track UUID")
    ),
    request_body = SetTrackQualityRequest,
    responses(
        (status = 204, description = "Track quality updated"),
        (status = 404, description = "Track not found"),
        (status = 500, description = "Failed to update track quality"),
    )
)]
async fn set_track_quality(
    State(state): State<AppState>,
    Path((album_id, track_id)): Path<(String, String)>,
    Json(request): Json<SetTrackQualityRequest>,
) -> ApiStatusResult {
    let album_id = parse_album_id(&album_id)?;
    let track_id = parse_track_id(&track_id)?;
    require_album(&state, album_id).await?;
    require_track(&state, album_id, track_id).await?;

    dispatch_action_impl(
        state,
        ServerAction::SetTrackQuality {
            album_id,
            track_id,
            quality: request.quality,
        },
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Add Album Artist
///
/// Associates an existing local artist with a local album.
#[utoipa::path(
    post,
    path = "/{album_id}/artist",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID")
    ),
    request_body = AlbumArtistRequest,
    responses(
        (status = 204, description = "Artist added to album"),
        (status = 404, description = "Album or artist not found"),
        (status = 500, description = "Failed to add artist to album"),
    )
)]
async fn add_album_artist(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
    Json(request): Json<AlbumArtistRequest>,
) -> ApiStatusResult {
    let album_id = parse_album_id(&album_id)?;
    require_album(&state, album_id).await?;
    require_artist(&state, request.artist_id).await?;

    dispatch_action_impl(
        state,
        ServerAction::AddAlbumArtist {
            album_id,
            artist_id: request.artist_id,
        },
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Remove Album Artist
///
/// Removes an associated local artist from a local album.
#[utoipa::path(
    delete,
    path = "/{album_id}/artist/{artist_id}",
    tag = TAG,
    params(
        ("album_id" = String, Path, description = "Album UUID"),
        ("artist_id" = String, Path, description = "Artist UUID")
    ),
    responses(
        (status = 204, description = "Artist removed from album"),
        (status = 404, description = "Album or artist not found"),
        (status = 409, description = "Cannot remove the only artist from an album"),
        (status = 500, description = "Failed to remove artist from album"),
    )
)]
async fn remove_album_artist(
    State(state): State<AppState>,
    Path((album_id, artist_id)): Path<(String, String)>,
) -> ApiStatusResult {
    let album_id = parse_album_id(&album_id)?;
    let artist_id = parse_artist_id(&artist_id)?;
    require_album(&state, album_id).await?;
    require_artist(&state, artist_id).await?;

    dispatch_action_impl(
        state,
        ServerAction::RemoveAlbumArtist {
            album_id,
            artist_id,
        },
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

fn parse_album_id(raw: &str) -> Result<Uuid, ApiErrorResponse> {
    parse_uuid(raw, "album_id")
}

fn parse_artist_id(raw: &str) -> Result<Uuid, ApiErrorResponse> {
    parse_uuid(raw, "artist_id")
}

fn parse_track_id(raw: &str) -> Result<Uuid, ApiErrorResponse> {
    parse_uuid(raw, "track_id")
}

fn parse_uuid(raw: &str, field: &'static str) -> Result<Uuid, ApiErrorResponse> {
    parse_uuid_param(raw, field)
}

async fn require_album(
    state: &AppState,
    album_id: Uuid,
) -> Result<MonitoredAlbum, ApiErrorResponse> {
    let albums = state.monitored_albums.read().await;
    albums
        .iter()
        .find(|album| album.id == album_id)
        .cloned()
        .ok_or_else(|| app_error_response(AppError::not_found("album", Some(album_id.to_string()))))
}

async fn require_artist(
    state: &AppState,
    artist_id: Uuid,
) -> Result<MonitoredArtist, ApiErrorResponse> {
    let artists = state.monitored_artists.read().await;
    artists
        .iter()
        .find(|artist| artist.id == artist_id)
        .cloned()
        .ok_or_else(|| {
            app_error_response(AppError::not_found("artist", Some(artist_id.to_string())))
        })
}

async fn require_track(
    state: &AppState,
    album_id: Uuid,
    track_id: Uuid,
) -> Result<TrackInfo, ApiErrorResponse> {
    let ctx = build_server_context(state);
    let tracks = (ctx.fetch_tracks)(album_id)
        .await
        .map_err(yoink_error_response)?;
    tracks
        .into_iter()
        .find(|track| track.id == track_id)
        .ok_or_else(|| app_error_response(AppError::not_found("track", Some(track_id.to_string()))))
}
