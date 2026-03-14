use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use yoink_shared::{DownloadJob, MonitoredAlbum, MonitoredArtist, TrackInfo};

use crate::{server_context::build_server_context, state::AppState};

use super::helpers::{ApiErrorResponse, yoink_error_response};

pub(crate) const TAG: &str = "Wanted";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for the wanted albums and tracks view";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;

#[derive(Debug, Clone, Serialize, ToSchema)]
struct WantedAlbumWithTracks {
    album: MonitoredAlbum,
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
async fn get_wanted(State(state): State<AppState>) -> ApiResult<WantedData> {
    let artists = state.monitored_artists.read().await.clone();
    let jobs = state.download_jobs.read().await.clone();
    let wanted_albums: Vec<MonitoredAlbum> = state
        .monitored_albums
        .read()
        .await
        .iter()
        .filter(|album| album.wanted || album.partially_wanted)
        .cloned()
        .collect();

    let ctx = build_server_context(&state);
    let mut albums = Vec::with_capacity(wanted_albums.len());
    for album in wanted_albums {
        let tracks = (ctx.fetch_tracks)(album.id)
            .await
            .map_err(yoink_error_response)?;
        albums.push(WantedAlbumWithTracks { album, tracks });
    }

    Ok(Json(WantedData {
        albums,
        artists,
        jobs,
    }))
}
