use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use yoink_shared::{DownloadJob, MonitoredAlbum, MonitoredArtist};

use crate::state::AppState;

use super::helpers::ApiErrorResponse;

pub(crate) const TAG: &str = "Dashboard";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for dashboard overview data";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;

#[derive(Debug, Clone, Serialize, ToSchema)]
struct DashboardData {
    artists: Vec<MonitoredArtist>,
    albums: Vec<MonitoredAlbum>,
    jobs: Vec<DownloadJob>,
}

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_dashboard))
}

/// Get Dashboard
///
/// Returns the dashboard overview payload with local artists, albums, and download jobs.
#[utoipa::path(
    get,
    path = "/",
    tag = TAG,
    responses(
        (status = 200, description = "Dashboard data", body = DashboardData),
    )
)]
async fn get_dashboard(State(state): State<AppState>) -> ApiResult<DashboardData> {
    Ok(Json(DashboardData {
        artists: state.monitored_artists.read().await.clone(),
        albums: state.monitored_albums.read().await.clone(),
        jobs: state.download_jobs.read().await.clone(),
    }))
}
