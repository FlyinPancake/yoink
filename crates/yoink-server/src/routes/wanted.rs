use std::collections::{HashMap, HashSet};

use axum::{Json, extract::State};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
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

async fn primary_artist_ids_by_album(
    state: &AppState,
    album_ids: &[Uuid],
) -> Result<HashMap<Uuid, Uuid>, ApiErrorResponse> {
    let mut primary_artist_ids = HashMap::new();
    if album_ids.is_empty() {
        return Ok(primary_artist_ids);
    }

    let junctions = db::album_artist::Entity::find_by_album_ids_ordered(album_ids.iter().copied())
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

async fn tracks_by_album(
    state: &AppState,
    album_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<TrackInfo>>, ApiErrorResponse> {
    let mut tracks_by_album = HashMap::new();
    if album_ids.is_empty() {
        return Ok(tracks_by_album);
    }

    let tracks = db::track::Entity::find_by_album_ids_ordered(album_ids.iter().copied())
        .all(&state.db)
        .await
        .map_err(|err| app_error_response(err.into()))?;

    for track in tracks {
        tracks_by_album
            .entry(track.album_id)
            .or_insert_with(Vec::new)
            .push(track.into_ex().into());
    }

    Ok(tracks_by_album)
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

    let album_ids: Vec<Uuid> = raw_albums.iter().map(|album| album.id).collect();
    let primary_artist_ids = primary_artist_ids_by_album(&state, &album_ids).await?;
    let tracks_by_album = tracks_by_album(&state, &album_ids).await?;

    let mut wanted_albums = Vec::with_capacity(raw_albums.len());
    let mut artist_ids = HashSet::new();

    for album in raw_albums {
        let primary_artist_id = primary_artist_ids.get(&album.id).copied();

        if let Some(artist_id) = primary_artist_id {
            artist_ids.insert(artist_id);
        }

        wanted_albums.push(WantedAlbumWithTracks {
            album: WantedAlbumSummary {
                artist_id: primary_artist_id,
                album: album.clone().into(),
            },
            tracks: tracks_by_album.get(&album.id).cloned().unwrap_or_default(),
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
