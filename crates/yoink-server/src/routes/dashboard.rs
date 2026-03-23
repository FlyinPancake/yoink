use axum::{Json, extract::State};
use sea_orm::EntityTrait;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use yoink_shared::{Album, DownloadJob, MonitoredArtist};

use crate::{db, routes::helpers::app_error_response, state::AppState};

use super::helpers::ApiErrorResponse;

pub(crate) const TAG: &str = "Dashboard";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for dashboard overview data";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;

#[derive(Debug, Clone, Serialize, ToSchema)]
struct DashboardData {
    artists: Vec<MonitoredArtist>,
    albums: Vec<Album>,
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
    let artists = db::artist::Entity::find()
        .all(&state.db)
        .await
        .map_err(|e| app_error_response(e.into()))?
        .into_iter()
        .map(Into::into)
        .collect();

    let albums = db::album::Entity::find()
        .all(&state.db)
        .await
        .map_err(|e| app_error_response(e.into()))?
        .into_iter()
        .map(Into::into)
        .collect();

    Ok(Json(DashboardData {
        artists,
        albums,
        jobs: vec![], // FIXME: implement download jobs and include here
    }))
}
