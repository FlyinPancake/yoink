use std::collections::HashSet;

use axum::{Json, extract::State};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use yoink_shared::{Album, DownloadJob, MonitoredArtist, TrackInfo};

use crate::{
    db::{self, wanted_status::WantedStatus},
    routes::helpers::app_error_response,
    services,
    state::AppState,
};

use super::helpers::ApiErrorResponse;

pub(crate) const TAG: &str = "Wanted";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for the wanted albums and tracks view";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;

#[derive(Debug, Clone, Serialize, ToSchema)]
struct WantedAlbumSummary {
    #[serde(flatten)]
    album: Album,
    artist_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
struct WantedAlbumWithTracks {
    album: WantedAlbumSummary,
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
    let mut album_ids: HashSet<Uuid> = db::album::Entity::find()
        .filter(db::album::Column::WantedStatus.ne(WantedStatus::Unmonitored))
        .all(&state.db)
        .await
        .map_err(|err| app_error_response(err.into()))?
        .into_iter()
        .map(|album| album.id)
        .collect();

    album_ids.extend(
        db::track::Entity::find()
            .filter(db::track::Column::Status.ne(WantedStatus::Unmonitored))
            .all(&state.db)
            .await
            .map_err(|err| app_error_response(err.into()))?
            .into_iter()
            .map(|track| track.album_id),
    );

    let raw_albums = if album_ids.is_empty() {
        Vec::new()
    } else {
        db::album::Entity::find()
            .filter(db::album::Column::Id.is_in(album_ids.iter().copied()))
            .all(&state.db)
            .await
            .map_err(|err| app_error_response(err.into()))?
    };

    let mut wanted_albums = Vec::with_capacity(raw_albums.len());
    let mut artist_ids = HashSet::new();

    for album in raw_albums {
        let primary_artist_id = db::album_artist::Entity::find()
            .filter(db::album_artist::Column::AlbumId.eq(album.id))
            .order_by_asc(db::album_artist::Column::Priority)
            .one(&state.db)
            .await
            .map_err(|err| app_error_response(err.into()))?
            .map(|junction| junction.artist_id);

        if let Some(artist_id) = primary_artist_id {
            artist_ids.insert(artist_id);
        }

        wanted_albums.push(WantedAlbumWithTracks {
            album: WantedAlbumSummary {
                artist_id: primary_artist_id,
                album: album.clone().into(),
            },
            tracks: services::album::get_album_tracks(&state.db, album.id)
                .await
                .map_err(app_error_response)?,
        });
    }

    let artists = if artist_ids.is_empty() {
        Vec::new()
    } else {
        db::artist::Entity::find()
            .filter(db::artist::Column::Id.is_in(artist_ids))
            .all(&state.db)
            .await
            .map_err(|err| app_error_response(err.into()))?
            .into_iter()
            .map(Into::into)
            .collect()
    };

    let jobs = services::downloads::list_jobs(&state)
        .await
        .map_err(app_error_response)?;

    Ok(Json(WantedData {
        albums: wanted_albums,
        artists,
        jobs,
    }))
}
