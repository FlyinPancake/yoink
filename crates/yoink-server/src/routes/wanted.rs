use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use yoink_shared::{Album, DownloadJob, MonitoredArtist, TrackInfo};

use crate::state::AppState;

use super::helpers::ApiErrorResponse;

pub(crate) const TAG: &str = "Wanted";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for the wanted albums and tracks view";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;

#[derive(Debug, Clone, Serialize, ToSchema)]
struct WantedAlbumWithTracks {
    album: Album,
    tracks: Vec<TrackInfo>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
struct WantedData {
    albums: Vec<WantedAlbumWithTracks>,
    artists: Vec<MonitoredArtist>,
    jobs: Vec<DownloadJob>,
}

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_wanted))
}

/// Get Wanted
///
/// Returns wanted and partially wanted albums together with their tracks, artists, and jobs.
#[utoipa::path(
    get,
    path = "/",
    tag = TAG,
    responses(
        (status = 200, description = "Wanted view data", body = WantedData),
        (status = 500, description = "Failed to load wanted data"),
    )
)]
async fn get_wanted(State(_state): State<AppState>) -> ApiResult<WantedData> {
    // FIXME: This is a placeholder implementation. We need to load the actual wanted albums, artists, and jobs from the database.

    let artists = vec![];
    let jobs = vec![];
    let albums = vec![];

    Ok(Json(WantedData {
        albums,
        artists,
        jobs,
    }))
}
