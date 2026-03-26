use axum::{Json, extract::State};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use yoink_shared::{Album, DownloadJob, MonitoredArtist};

use crate::{db, routes::helpers::app_error_response, services, state::AppState};

use super::helpers::ApiErrorResponse;

pub(crate) const TAG: &str = "Dashboard";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for dashboard overview data";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;

#[derive(Debug, Clone, Serialize, ToSchema)]
struct DashboardAlbum {
    #[serde(flatten)]
    album: Album,
    artist_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
struct DashboardData {
    artists: Vec<MonitoredArtist>,
    albums: Vec<DashboardAlbum>,
    jobs: Vec<DownloadJob>,
}

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_dashboard))
}

async fn primary_artist_id(
    state: &AppState,
    album_id: Uuid,
) -> Result<Option<Uuid>, ApiErrorResponse> {
    db::album_artist::Entity::find()
        .filter(db::album_artist::Column::AlbumId.eq(album_id))
        .order_by_asc(db::album_artist::Column::Priority)
        .one(&state.db)
        .await
        .map_err(|err| app_error_response(err.into()))
        .map(|junction| junction.map(|junction| junction.artist_id))
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
        .map_err(|err| app_error_response(err.into()))?
        .into_iter()
        .map(Into::into)
        .collect();

    let raw_albums = db::album::Entity::find()
        .order_by_desc(db::album::Column::ModifiedAt)
        .all(&state.db)
        .await
        .map_err(|err| app_error_response(err.into()))?;

    let mut albums = Vec::with_capacity(raw_albums.len());
    for album in raw_albums {
        albums.push(DashboardAlbum {
            artist_id: primary_artist_id(&state, album.id).await?,
            album: album.into(),
        });
    }

    let jobs = services::downloads::list_jobs(&state)
        .await
        .map_err(app_error_response)?;

    Ok(Json(DashboardData {
        artists,
        albums,
        jobs,
    }))
}
