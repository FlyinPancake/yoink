use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::api::{Album, ProviderLink, SearchAlbumResult, TrackInfo};

use crate::{
    db::{self, provider::Provider, quality::Quality},
    error::AppError,
    services::{self, album::AlbumDetailResponse, search::SearchQuery},
    state::AppState,
};

use super::helpers::{ApiErrorResponse, app_error_response};

pub(crate) const TAG: &str = "Album";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for album search, lookup, and lifecycle";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;
type ApiStatusResult = Result<StatusCode, ApiErrorResponse>;

#[derive(Debug, Deserialize, ToSchema)]
struct CreateAlbumRequest {
    provider: Provider,
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

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
struct LibraryAlbumSummary {
    #[serde(flatten)]
    album: Album,
    artist_id: Option<Uuid>,
    artist_name: Option<String>,
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

// ── Helpers ──────────────────────────────────────────────────────────

async fn require_album(state: &AppState, album_id: Uuid) -> Result<(), ApiErrorResponse> {
    db::album::Entity::find_by_id(album_id)
        .one(&state.db)
        .await
        .map_err(|e| app_error_response(e.into()))?
        .ok_or_else(|| {
            app_error_response(AppError::not_found("album", Some(album_id.to_string())))
        })?;
    Ok(())
}

async fn require_artist(state: &AppState, artist_id: Uuid) -> Result<(), ApiErrorResponse> {
    db::artist::Entity::find_by_id(artist_id)
        .one(&state.db)
        .await
        .map_err(|e| app_error_response(e.into()))?
        .ok_or_else(|| {
            app_error_response(AppError::not_found("artist", Some(artist_id.to_string())))
        })?;
    Ok(())
}

async fn require_track(state: &AppState, track_id: Uuid) -> Result<(), ApiErrorResponse> {
    db::track::Entity::find_by_id(track_id)
        .one(&state.db)
        .await
        .map_err(|e| app_error_response(e.into()))?
        .ok_or_else(|| {
            app_error_response(AppError::not_found("track", Some(track_id.to_string())))
        })?;
    Ok(())
}

// ── Routes ───────────────────────────────────────────────────────────

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
    Query(query): Query<SearchQuery>,
) -> ApiResult<Vec<SearchAlbumResult>> {
    services::search::search_albums(&state.db, &state.registry, &query)
        .await
        .map_err(app_error_response)
        .map(Json)
}

/// List Albums
///
/// Returns all locally stored albums from the library database.
#[utoipa::path(
    get,
    path = "/",
    tag = TAG,
    responses(
        (status = 200, description = "All local albums", body = Vec<LibraryAlbumSummary>),
        (status = 500, description = "Failed to load albums"),
    )
)]
async fn list_albums(State(state): State<AppState>) -> ApiResult<Vec<LibraryAlbumSummary>> {
    let raw_albums = db::album::Entity::find()
        .all(&state.db)
        .await
        .map_err(|e| app_error_response(e.into()))?;

    let album_ids: Vec<Uuid> = raw_albums.iter().map(|album| album.id).collect();
    let album_artists =
        db::album_artist::Entity::find_by_album_ids_ordered(album_ids.iter().copied())
            .all(&state.db)
            .await
            .map_err(|e| app_error_response(e.into()))?;

    let mut primary_artist_ids = std::collections::HashMap::new();
    for junction in album_artists {
        primary_artist_ids
            .entry(junction.album_id)
            .or_insert(junction.artist_id);
    }

    let artist_ids: Vec<Uuid> = primary_artist_ids.values().copied().collect();
    let artists_by_id: std::collections::HashMap<Uuid, db::artist::Model> = if artist_ids.is_empty()
    {
        std::collections::HashMap::new()
    } else {
        db::artist::Entity::find()
            .filter(db::artist::Column::Id.is_in(artist_ids.iter().copied()))
            .all(&state.db)
            .await
            .map_err(|e| app_error_response(e.into()))?
            .into_iter()
            .map(|artist| (artist.id, artist))
            .collect()
    };

    let albums = raw_albums
        .into_iter()
        .map(|album| {
            let artist_id = primary_artist_ids.get(&album.id).copied();
            let artist_name =
                artist_id.and_then(|id| artists_by_id.get(&id).map(|artist| artist.name.clone()));

            LibraryAlbumSummary {
                album: album.into(),
                artist_id,
                artist_name,
            }
        })
        .collect();

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
    services::album::add_album(
        &state,
        request.provider,
        request.external_album_id,
        request.artist_external_id,
        request.artist_name,
        request.monitor_all,
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

    services::album::merge_albums(
        &state,
        request.target_album_id,
        request.source_album_id,
        request.result_title,
        request.result_cover_url,
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
        ("album_id" = Uuid, Path, description = "Album UUID")
    ),
    responses(
        (status = 200, description = "Album detail payload", body = AlbumDetailResponse),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to load album detail"),
    )
)]
async fn get_album(
    State(state): State<AppState>,
    Path(album_id): Path<Uuid>,
) -> ApiResult<AlbumDetailResponse> {
    let res = services::album::get_album_details(&state, album_id)
        .await
        .map_err(app_error_response)?;
    Ok(Json(res))
}

/// Toggle Album Monitor
///
/// Enables or disables album-level monitoring for a local album.
#[utoipa::path(
    patch,
    path = "/{album_id}/monitor",
    tag = TAG,
    params(
        ("album_id" = Uuid, Path, description = "Album UUID")
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
    Path(album_id): Path<Uuid>,
    Json(request): Json<ToggleAlbumMonitorRequest>,
) -> ApiStatusResult {
    require_album(&state, album_id).await?;

    services::album::toggle_album_monitor(&state, album_id, request.monitored)
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
        ("album_id" = Uuid, Path, description = "Album UUID")
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
    Path(album_id): Path<Uuid>,
    Json(request): Json<SetAlbumQualityRequest>,
) -> ApiStatusResult {
    require_album(&state, album_id).await?;

    services::album::set_album_quality(&state, album_id, request.quality)
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
        ("album_id" = Uuid, Path, description = "Album UUID"),
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
    Path(album_id): Path<Uuid>,
    Query(query): Query<RemoveAlbumFilesQuery>,
) -> ApiStatusResult {
    require_album(&state, album_id).await?;

    services::album::remove_album_files(&state, album_id, query.unmonitor)
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
        ("album_id" = Uuid, Path, description = "Album UUID")
    ),
    responses(
        (status = 204, description = "Album download queued"),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to retry album download"),
    )
)]
async fn retry_download(
    State(state): State<AppState>,
    Path(album_id): Path<Uuid>,
) -> ApiStatusResult {
    require_album(&state, album_id).await?;

    crate::services::downloads::retry_album_download(&state, album_id)
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
        ("album_id" = Uuid, Path, description = "Album UUID")
    ),
    responses(
        (status = 200, description = "Provider links for the album", body = Vec<ProviderLink>),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to load album providers"),
    )
)]
async fn list_album_providers(
    State(state): State<AppState>,
    Path(album_id): Path<Uuid>,
) -> ApiResult<Vec<ProviderLink>> {
    require_album(&state, album_id).await?;
    let links = db::album_provider_link::Entity::find()
        .filter(db::album_provider_link::Column::AlbumId.eq(album_id))
        .all(&state.db)
        .await
        .map_err(|err| app_error_response(err.into()))?
        .into_iter()
        .map(Into::into)
        .collect();
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
        ("album_id" = Uuid, Path, description = "Album UUID")
    ),
    responses(
        (status = 200, description = "Tracks for the album", body = Vec<TrackInfo>),
        (status = 404, description = "Album not found"),
        (status = 500, description = "Failed to load album tracks"),
    )
)]
async fn list_album_tracks(
    State(state): State<AppState>,
    Path(album_id): Path<Uuid>,
) -> ApiResult<Vec<TrackInfo>> {
    services::album::get_album_tracks(&state.db, album_id)
        .await
        .map_err(app_error_response)
        .map(Json)
}

/// Bulk Toggle Track Monitor
///
/// Enables or disables monitoring for every track on a local album.
#[utoipa::path(
    patch,
    path = "/{album_id}/track/monitor",
    tag = TAG,
    params(
        ("album_id" = Uuid, Path, description = "Album UUID")
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
    Path(album_id): Path<Uuid>,
    Json(request): Json<ToggleTrackMonitorRequest>,
) -> ApiStatusResult {
    require_album(&state, album_id).await?;

    services::track::bulk_toggle_track_monitor(&state, album_id, request.monitored)
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
        ("album_id" = Uuid, Path, description = "Album UUID"),
        ("track_id" = Uuid, Path, description = "Track UUID")
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
    Path((album_id, track_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<ToggleTrackMonitorRequest>,
) -> ApiStatusResult {
    require_album(&state, album_id).await?;
    require_track(&state, track_id).await?;

    services::track::toggle_track_monitor(&state, track_id, album_id, request.monitored)
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
        ("album_id" = Uuid, Path, description = "Album UUID"),
        ("track_id" = Uuid, Path, description = "Track UUID")
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
    Path((album_id, track_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<SetTrackQualityRequest>,
) -> ApiStatusResult {
    require_album(&state, album_id).await?;
    require_track(&state, track_id).await?;

    services::track::set_track_quality(&state, album_id, track_id, request.quality)
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
        ("album_id" = Uuid, Path, description = "Album UUID")
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
    Path(album_id): Path<Uuid>,
    Json(request): Json<AlbumArtistRequest>,
) -> ApiStatusResult {
    require_album(&state, album_id).await?;
    require_artist(&state, request.artist_id).await?;

    services::album::add_album_artist(&state, album_id, request.artist_id)
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
        ("album_id" = Uuid, Path, description = "Album UUID"),
        ("artist_id" = Uuid, Path, description = "Artist UUID")
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
    Path((album_id, artist_id)): Path<(Uuid, Uuid)>,
) -> ApiStatusResult {
    require_album(&state, album_id).await?;
    require_artist(&state, artist_id).await?;

    services::album::remove_album_artist(&state, album_id, artist_id)
        .await
        .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    use tower::ServiceExt as _;

    use crate::{
        app_config::AuthConfig, providers::registry::ProviderRegistry, routes::build_router,
        state::AppState,
    };

    use super::*;

    async fn test_state() -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-album-route-test-{}.db?mode=rwc",
            uuid::Uuid::now_v7()
        );

        AppState::new(
            std::path::PathBuf::from("./music"),
            crate::db::quality::Quality::Lossless,
            false,
            1,
            &db_path,
            ProviderRegistry::new(),
            AuthConfig {
                enabled: false,
                session_secret: String::new(),
                init_admin_username: None,
                init_admin_password: None,
            },
        )
        .await
    }

    #[tokio::test]
    async fn list_albums_returns_primary_artist_context() {
        let state = test_state().await;
        let artist = db::artist::ActiveModel {
            name: Set("Primary Artist".to_string()),
            monitored: Set(true),
            ..Default::default()
        }
        .insert(&state.db)
        .await
        .expect("artist");
        let album = db::album::ActiveModel {
            title: Set("Album One".to_string()),
            album_type: Set(db::album_type::AlbumType::Album),
            explicit: Set(false),
            wanted_status: Set(db::wanted_status::WantedStatus::Wanted),
            requested_quality: Set(Some(Quality::Lossless)),
            ..Default::default()
        }
        .insert(&state.db)
        .await
        .expect("album");
        db::album_artist::ActiveModel {
            album_id: Set(album.id),
            artist_id: Set(artist.id),
            priority: Set(0),
        }
        .insert(&state.db)
        .await
        .expect("album artist");

        let app = build_router(state).split_for_parts().0;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/album")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: Vec<LibraryAlbumSummary> = serde_json::from_slice(&body).expect("json");

        assert_eq!(payload.len(), 1);
        assert_eq!(payload[0].artist_id, Some(artist.id));
        assert_eq!(payload[0].artist_name.as_deref(), Some("Primary Artist"));
        assert_eq!(
            payload[0].album.quality_override,
            Some(crate::api::Quality::Lossless)
        );
        assert_eq!(
            payload[0].album.wanted_status,
            crate::api::WantedStatus::Wanted
        );
    }

    #[tokio::test]
    async fn list_albums_uses_first_ordered_artist_when_multiple_artists_exist() {
        let state = test_state().await;
        let first_artist = db::artist::ActiveModel {
            name: Set("First Artist".to_string()),
            monitored: Set(true),
            ..Default::default()
        }
        .insert(&state.db)
        .await
        .expect("first artist");
        let second_artist = db::artist::ActiveModel {
            name: Set("Second Artist".to_string()),
            monitored: Set(true),
            ..Default::default()
        }
        .insert(&state.db)
        .await
        .expect("second artist");
        let album = db::album::ActiveModel {
            title: Set("Album Two".to_string()),
            album_type: Set(db::album_type::AlbumType::Album),
            explicit: Set(false),
            wanted_status: Set(db::wanted_status::WantedStatus::Acquired),
            requested_quality: Set(None),
            ..Default::default()
        }
        .insert(&state.db)
        .await
        .expect("album");

        db::album_artist::ActiveModel {
            album_id: Set(album.id),
            artist_id: Set(first_artist.id),
            priority: Set(0),
        }
        .insert(&state.db)
        .await
        .expect("primary junction");
        db::album_artist::ActiveModel {
            album_id: Set(album.id),
            artist_id: Set(second_artist.id),
            priority: Set(1),
        }
        .insert(&state.db)
        .await
        .expect("secondary junction");

        let app = build_router(state).split_for_parts().0;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/album")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: Vec<LibraryAlbumSummary> = serde_json::from_slice(&body).expect("json");

        assert_eq!(payload.len(), 1);
        assert_eq!(payload[0].artist_id, Some(first_artist.id));
        assert_eq!(payload[0].artist_name.as_deref(), Some("First Artist"));
        assert_eq!(
            payload[0].album.wanted_status,
            crate::api::WantedStatus::Acquired
        );
    }

    #[tokio::test]
    async fn list_albums_returns_none_artist_fields_when_album_has_no_artists() {
        let state = test_state().await;
        db::album::ActiveModel {
            title: Set("Orphan Album".to_string()),
            album_type: Set(db::album_type::AlbumType::Album),
            explicit: Set(false),
            wanted_status: Set(db::wanted_status::WantedStatus::Unmonitored),
            requested_quality: Set(None),
            ..Default::default()
        }
        .insert(&state.db)
        .await
        .expect("album");

        let app = build_router(state).split_for_parts().0;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/album")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: Vec<LibraryAlbumSummary> = serde_json::from_slice(&body).expect("json");

        assert_eq!(payload.len(), 1);
        assert_eq!(payload[0].artist_id, None);
        assert_eq!(payload[0].artist_name, None);
        assert_eq!(
            payload[0].album.wanted_status,
            crate::api::WantedStatus::Unmonitored
        );
    }
}
