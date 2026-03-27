use std::collections::HashMap;

use axum::{Json, extract::State};
use sea_orm::{EntityTrait, QueryOrder};
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

async fn primary_artist_ids_by_album(
    state: &AppState,
    album_ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, Uuid>, ApiErrorResponse> {
    let mut primary_artist_ids = HashMap::new();
    if album_ids.is_empty() {
        return Ok(primary_artist_ids);
    }

    let junctions = db::album_artist::Entity::find_by_album_ids_ordered(album_ids)
        .all(&state.db)
        .await
        .map_err(|err| app_error_response(err.into()))?;

    for junction in junctions {
        primary_artist_ids
            .entry(junction.album_id)
            .or_insert(junction.artist_id);
    }

    Ok(primary_artist_ids)
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
    let primary_artist_ids =
        primary_artist_ids_by_album(&state, raw_albums.iter().map(|album| album.id).collect())
            .await?;

    let mut albums = Vec::with_capacity(raw_albums.len());
    for album in raw_albums {
        albums.push(DashboardAlbum {
            artist_id: primary_artist_ids.get(&album.id).copied(),
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
