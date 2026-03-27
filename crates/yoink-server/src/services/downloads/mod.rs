mod io;
mod lyrics;
mod metadata;
mod worker;

pub(crate) use io::sanitize_path_component;
pub(crate) use metadata::{TrackMetadata, write_audio_metadata};
use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
};

use sea_orm::{
    ActiveEnum, ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityLoaderTrait, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder,
};
use tokio::fs;
use uuid::Uuid;
use yoink_shared::{DownloadJob, DownloadJobKind};

use crate::{
    db::{
        self, download_job, download_status::DownloadStatus, provider::Provider,
        wanted_status::WantedStatus,
    },
    error::{AppError, AppResult},
    services::downloads::worker::{download_album_job, download_track_job},
    state::AppState,
};

fn in_progress_statuses() -> [DownloadStatus; 3] {
    [
        DownloadStatus::Queued,
        DownloadStatus::Resolving,
        DownloadStatus::Downloading,
    ]
}

fn history_statuses() -> [DownloadStatus; 2] {
    [DownloadStatus::Completed, DownloadStatus::Failed]
}

fn actively_running_statuses() -> [DownloadStatus; 2] {
    [DownloadStatus::Resolving, DownloadStatus::Downloading]
}

fn select_download_source<I>(state: &AppState, providers: I) -> Provider
where
    I: IntoIterator<Item = Provider>,
{
    let download_sources = state.registry.download_sources();
    let download_source_ids: HashSet<_> =
        download_sources.iter().map(|source| source.id()).collect();
    let providers: Vec<_> = providers.into_iter().collect();

    if let Some(provider) = providers
        .iter()
        .find(|provider| download_source_ids.contains(provider))
    {
        return *provider;
    }

    download_sources
        .iter()
        .find(|source| !source.requires_linked_provider())
        .map(|source| source.id())
        .or_else(|| download_source_ids.iter().next().copied())
        .unwrap_or(Provider::Tidal)
}

async fn primary_artist_names_by_album_ids(
    state: &AppState,
    album_ids: impl IntoIterator<Item = Uuid>,
) -> AppResult<HashMap<Uuid, String>> {
    let album_ids: Vec<Uuid> = album_ids.into_iter().collect();
    if album_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let primary_junctions = db::album_artist::Entity::find_by_album_ids_ordered(album_ids)
        .all(&state.db)
        .await?;
    let mut album_to_artist_id = HashMap::new();
    let mut artist_ids = HashSet::new();

    for junction in primary_junctions {
        if album_to_artist_id
            .insert(junction.album_id, junction.artist_id)
            .is_none()
        {
            artist_ids.insert(junction.artist_id);
        }
    }

    if artist_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let artist_names: HashMap<Uuid, String> = db::artist::Entity::find()
        .filter(db::artist::Column::Id.is_in(artist_ids))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|artist| (artist.id, artist.name))
        .collect();

    Ok(album_to_artist_id
        .into_iter()
        .filter_map(|(album_id, artist_id)| {
            artist_names
                .get(&artist_id)
                .cloned()
                .map(|artist_name| (album_id, artist_name))
        })
        .collect())
}

async fn serialize_job(
    job: download_job::ModelEx,
    artist_names_by_album: &HashMap<Uuid, String>,
) -> AppResult<DownloadJob> {
    let album_title = job
        .album
        .as_ref()
        .map(|album| album.title.clone())
        .unwrap_or_default();
    let track_title = job.track.as_ref().map(|track| track.title.clone());
    let artist_name = artist_names_by_album
        .get(&job.album_id)
        .cloned()
        .unwrap_or_default();

    Ok(DownloadJob {
        id: job.id,
        kind: if job.track_id.is_some() {
            DownloadJobKind::Track
        } else {
            DownloadJobKind::Album
        },
        album_id: job.album_id,
        track_id: job.track_id,
        source: job.source.to_value(),
        album_title,
        track_title,
        artist_name,
        status: job.status.into(),
        quality: job.quality.into(),
        total_tracks: job.total_tracks,
        completed_tracks: job.completed_tasks,
        error: job.error_message,
        created_at: job.created_at,
        updated_at: job.modified_at,
    })
}

async fn serialize_jobs(
    jobs: Vec<download_job::ModelEx>,
    artist_names_by_album: &HashMap<Uuid, String>,
) -> AppResult<Vec<DownloadJob>> {
    let mut out = Vec::with_capacity(jobs.len());
    for job in jobs {
        out.push(serialize_job(job, artist_names_by_album).await?);
    }
    Ok(out)
}

pub(crate) async fn list_jobs(state: &AppState) -> AppResult<Vec<DownloadJob>> {
    let jobs = download_job::Entity::load()
        .with(db::album::Entity)
        .with(db::track::Entity)
        .order_by_desc(download_job::Column::ModifiedAt)
        .all(&state.db)
        .await?;

    let artist_names_by_album =
        primary_artist_names_by_album_ids(state, jobs.iter().map(|job| job.album_id)).await?;

    serialize_jobs(jobs, &artist_names_by_album).await
}

pub(crate) async fn list_album_jobs(
    state: &AppState,
    album_id: Uuid,
) -> AppResult<Vec<DownloadJob>> {
    let jobs = download_job::Entity::load()
        .with(db::album::Entity)
        .with(db::track::Entity)
        .filter(download_job::Column::AlbumId.eq(album_id))
        .order_by_desc(download_job::Column::ModifiedAt)
        .all(&state.db)
        .await?;

    let artist_names_by_album =
        primary_artist_names_by_album_ids(state, jobs.iter().map(|job| job.album_id)).await?;

    serialize_jobs(jobs, &artist_names_by_album).await
}

pub(crate) async fn sync_album_wanted_status_from_tracks(
    state: &AppState,
    album_id: Uuid,
) -> AppResult<()> {
    let Some(album) = db::album::Entity::find_by_id(album_id)
        .one(&state.db)
        .await?
    else {
        return Err(AppError::not_found("album", Some(album_id.to_string())));
    };

    let tracks = db::track::Entity::find()
        .filter(db::track::Column::AlbumId.eq(album_id))
        .all(&state.db)
        .await?;

    let monitored_tracks = tracks
        .iter()
        .filter(|track| track.status != WantedStatus::Unmonitored)
        .count();

    let next_status = if monitored_tracks == 0 {
        Some(WantedStatus::Unmonitored)
    } else if monitored_tracks == tracks.len() && !tracks.is_empty() {
        Some(
            if tracks
                .iter()
                .all(|track| track.status == WantedStatus::Acquired)
            {
                WantedStatus::Acquired
            } else {
                WantedStatus::Wanted
            },
        )
    } else {
        None
    };

    if let Some(next_status) = next_status
        && next_status != album.wanted_status
    {
        let mut active = album.into_active_model();
        active.wanted_status = Set(next_status);
        active.update(&state.db).await?;
    }

    Ok(())
}

/// Enqueue a download job for an album.
pub(crate) async fn enqueue_album_download(state: &AppState, album_id: Uuid) -> AppResult<()> {
    let Some(album) = db::album::Entity::load()
        .filter_by_id(album_id)
        .with(db::download_job::Entity)
        .with(db::track::Entity)
        .with(db::album_provider_link::Entity)
        .one(&state.db)
        .await?
    else {
        return Err(AppError::not_found("album", Some(album_id.to_string())));
    };

    if album.wanted_status == WantedStatus::Unmonitored
        && album
            .tracks
            .iter()
            .all(|track| track.status == WantedStatus::Unmonitored)
    {
        return Err(AppError::download(
            "enqueue",
            "cannot enqueue a download for an unmonitored album",
        ));
    }

    if album
        .download_jobs
        .iter()
        .any(|job| job.status.in_progress())
    {
        return Err(AppError::validation(
            None::<String>,
            "a download job for this album is already in progress",
        ));
    }

    let quality = album.requested_quality.unwrap_or(state.default_quality);
    let source =
        select_download_source(state, album.provider_links.iter().map(|link| link.provider));
    let total_tracks = album.tracks.len() as i32;

    download_job::ActiveModel {
        album_id: Set(album.id),
        track_id: Set(None),
        source: Set(source),
        quality: Set(quality),
        total_tracks: Set(total_tracks),
        completed_tasks: Set(0),
        status: Set(DownloadStatus::Queued),
        error_message: Set(None),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    state.download_notify.notify_one();
    state.notify_sse();
    Ok(())
}

pub(crate) async fn enqueue_track_download(state: &AppState, track_id: Uuid) -> AppResult<()> {
    let Some(track) = db::track::Entity::find_by_id(track_id)
        .one(&state.db)
        .await?
    else {
        return Err(AppError::not_found("track", Some(track_id.to_string())));
    };

    if track.status == WantedStatus::Unmonitored {
        return Err(AppError::validation(
            None::<String>,
            "cannot enqueue a download for an unmonitored track",
        ));
    }

    if track.status == WantedStatus::Acquired || track.file_path.is_some() {
        return Ok(());
    }

    let Some(album) = db::album::Entity::load()
        .filter_by_id(track.album_id)
        .with(db::album_provider_link::Entity)
        .one(&state.db)
        .await?
    else {
        return Err(AppError::not_found(
            "album",
            Some(track.album_id.to_string()),
        ));
    };

    if download_job::Entity::find()
        .filter(download_job::Column::AlbumId.eq(track.album_id))
        .filter(download_job::Column::TrackId.is_null())
        .filter(download_job::Column::Status.is_in(in_progress_statuses()))
        .one(&state.db)
        .await?
        .is_some()
    {
        return Err(AppError::conflict(
            "an album download for this album is already in progress",
        ));
    }

    if download_job::Entity::find()
        .filter(download_job::Column::TrackId.eq(track.id))
        .filter(download_job::Column::Status.is_in(in_progress_statuses()))
        .one(&state.db)
        .await?
        .is_some()
    {
        return Err(AppError::validation(
            None::<String>,
            "a download job for this track is already in progress",
        ));
    }

    let quality = album.requested_quality.unwrap_or(state.default_quality);
    let source =
        select_download_source(state, album.provider_links.iter().map(|link| link.provider));

    download_job::ActiveModel {
        album_id: Set(track.album_id),
        track_id: Set(Some(track.id)),
        source: Set(source),
        quality: Set(quality),
        total_tracks: Set(1),
        completed_tasks: Set(0),
        status: Set(DownloadStatus::Queued),
        error_message: Set(None),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    state.download_notify.notify_one();
    state.notify_sse();
    Ok(())
}

pub(crate) async fn retry_album_download(state: &AppState, album_id: Uuid) -> AppResult<()> {
    enqueue_album_download(state, album_id).await
}

pub(crate) async fn prepare_album_for_unmonitor(state: &AppState, album_id: Uuid) -> AppResult<()> {
    if download_job::Entity::find()
        .filter(download_job::Column::AlbumId.eq(album_id))
        .filter(download_job::Column::Status.is_in(actively_running_statuses()))
        .one(&state.db)
        .await?
        .is_some()
    {
        return Err(AppError::conflict(
            "cannot unmonitor an album while a download is actively running",
        ));
    }

    let delete_result = download_job::Entity::delete_many()
        .filter(download_job::Column::AlbumId.eq(album_id))
        .filter(download_job::Column::Status.eq(DownloadStatus::Queued))
        .exec(&state.db)
        .await?;

    if delete_result.rows_affected > 0 {
        state.notify_sse();
    }

    Ok(())
}

pub(crate) async fn cancel_job(state: &AppState, job_id: Uuid) -> AppResult<()> {
    let Some(job) = download_job::Entity::find_by_id(job_id)
        .one(&state.db)
        .await?
    else {
        return Err(AppError::not_found(
            "download_job",
            Some(job_id.to_string()),
        ));
    };

    if job.status != DownloadStatus::Queued {
        return Err(AppError::validation(
            None::<String>,
            "only queued jobs can be cancelled",
        ));
    }

    download_job::Entity::delete_by_id(job_id)
        .exec(&state.db)
        .await?;
    state.notify_sse();
    Ok(())
}

pub(crate) async fn clear_completed_jobs(state: &AppState) -> AppResult<()> {
    download_job::Entity::delete_many()
        .filter(download_job::Column::Status.is_in(history_statuses()))
        .exec(&state.db)
        .await?;
    state.notify_sse();
    Ok(())
}

/// Background download worker loop.
pub(crate) async fn download_worker_loop(state: AppState) -> AppResult<()> {
    tracing::warn!("download worker started");
    loop {
        let Some(job) = download_job::Entity::load()
            .with(db::album::Entity)
            .with((db::album::Entity, db::album_provider_link::Entity))
            .with(db::track::Entity)
            .filter(download_job::Column::Status.eq(DownloadStatus::Queued))
            .order_by_asc(download_job::Column::CreatedAt)
            .one(&state.db)
            .await?
        else {
            state.download_notify.notified().await;
            continue;
        };

        let is_track_job = job.track_id.is_some();
        let album_id = job.album_id;
        let job_id = job.id;

        let job = job
            .into_active_model()
            .set_status(DownloadStatus::Resolving)
            .set_error_message(None)
            .update(&state.db)
            .await?;

        if !is_track_job
            && let Some(album) = job.album.as_ref()
            && album.wanted_status == WantedStatus::Wanted
        {
            let loaded_album_id = album.id;
            let mut active = album.clone().into_active_model();
            active.wanted_status = Set(WantedStatus::InProgress);
            if let Err(err) = active.update(&state.db).await {
                tracing::error!(
                    album_id = %loaded_album_id,
                    error = %err,
                    "Failed to update album wanted status to InProgress at start of download job"
                );
            }
        }

        state.notify_sse();

        tracing::info!(job_id = %job_id, album_id = %album_id, is_track_job, "Starting download job");

        let result = if is_track_job {
            download_track_job(&state, job.clone()).await
        } else {
            download_album_job(&state, job.clone()).await
        };

        match result {
            Ok(()) => {
                let mut job = match job
                    .into_active_model()
                    .set_status(DownloadStatus::Completed)
                    .set_error_message(None)
                    .update(&state.db)
                    .await
                {
                    Ok(job) => job,
                    Err(err) => {
                        tracing::error!(
                            job_id = %job_id,
                            error = %err,
                            "Failed to update download job status to Completed"
                        );
                        continue;
                    }
                };

                if is_track_job {
                    if let Err(err) = sync_album_wanted_status_from_tracks(&state, album_id).await {
                        tracing::error!(
                            album_id = %album_id,
                            error = %err,
                            "Failed to refresh album wanted status after track download"
                        );
                    }
                } else if let Some(album) = job.album.take()
                    && album.wanted_status != WantedStatus::Acquired
                {
                    let mut active = album.into_active_model();
                    active.wanted_status = Set(WantedStatus::Acquired);
                    if let Err(err) = active.update(&state.db).await {
                        tracing::error!(
                            album_id = %album_id,
                            error = %err,
                            "Failed to update album wanted status to Acquired after download"
                        );
                    }
                }
            }
            Err(err) => {
                tracing::error!(job_id = %job_id, error = %err, "Download job failed");

                let mut job = match job
                    .into_active_model()
                    .set_status(DownloadStatus::Failed)
                    .set_error_message(Some(err.to_string()))
                    .update(&state.db)
                    .await
                {
                    Ok(job) => job,
                    Err(update_err) => {
                        tracing::error!(
                            job_id = %job_id,
                            error = %update_err,
                            "Failed to update download job status to Failed after job failure"
                        );
                        continue;
                    }
                };

                if !is_track_job
                    && let Some(album) = job.album.take()
                    && album.wanted_status == WantedStatus::InProgress
                {
                    let mut active = album.into_active_model();
                    active.wanted_status = Set(WantedStatus::Wanted);
                    if let Err(update_err) = active.update(&state.db).await {
                        tracing::error!(
                            album_id = %album_id,
                            error = %update_err,
                            "Failed to update album wanted status back to Wanted after download job failure"
                        );
                    }
                }
            }
        }

        state.notify_sse();
    }
}

fn has_parent_dir_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn resolve_managed_track_path(music_root: &Path, stored_path: &str) -> Option<PathBuf> {
    let stored_path = Path::new(stored_path);

    if has_parent_dir_component(stored_path) {
        return None;
    }

    if stored_path.is_absolute() {
        return stored_path
            .starts_with(music_root)
            .then(|| stored_path.to_path_buf());
    }

    Some(music_root.join(stored_path))
}

async fn remove_file_if_exists(path: &Path) -> AppResult<bool> {
    if !fs::try_exists(path)
        .await
        .map_err(|err| AppError::filesystem("check file exists", path.display().to_string(), err))?
    {
        return Ok(false);
    }

    fs::remove_file(path)
        .await
        .map_err(|err| AppError::filesystem("remove file", path.display().to_string(), err))?;
    Ok(true)
}

async fn prune_empty_parent_dirs(path: &Path, music_root: &Path) -> AppResult<()> {
    let mut current = path.parent();

    while let Some(dir) = current {
        if dir == music_root {
            break;
        }

        let mut entries = match fs::read_dir(dir).await {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(err) => {
                return Err(AppError::filesystem(
                    "read directory",
                    dir.display().to_string(),
                    err,
                ));
            }
        };
        let is_empty = entries
            .next_entry()
            .await
            .map_err(|err| {
                AppError::filesystem("read directory entry", dir.display().to_string(), err)
            })?
            .is_none();

        if !is_empty {
            break;
        }

        match fs::remove_dir(dir).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(err) => {
                return Err(AppError::filesystem(
                    "remove directory",
                    dir.display().to_string(),
                    err,
                ));
            }
        }
        current = dir.parent();
    }

    Ok(())
}

/// Remove downloaded album files from disk.
pub(crate) async fn remove_downloaded_album_files(
    state: &AppState,
    album: &db::album::Model,
) -> AppResult<bool> {
    let tracks = db::track::Entity::find()
        .filter(db::track::Column::AlbumId.eq(album.id))
        .all(&state.db)
        .await?;

    let mut removed_any = false;
    let mut prunable_dirs = HashSet::new();

    for track in tracks {
        if let Some(file_path) = track.file_path.clone() {
            let Some(absolute_path) = resolve_managed_track_path(&state.music_root, &file_path)
            else {
                tracing::warn!(
                    album_id = %album.id,
                    track_id = %track.id,
                    file_path,
                    music_root = %state.music_root.display(),
                    "Skipping file removal for path outside managed music root"
                );
                continue;
            };

            let removed_audio = remove_file_if_exists(&absolute_path).await?;
            let removed_sidecar =
                remove_file_if_exists(&absolute_path.with_extension("lrc")).await?;

            if removed_audio || removed_sidecar {
                removed_any = true;
                prunable_dirs.insert(absolute_path);
            }
        }

        let was_acquired = track.status == WantedStatus::Acquired;
        let mut active = track.into_active_model();
        active.file_path = Set(None);
        active.root_folder_id = Set(None);
        if was_acquired {
            active.status = Set(WantedStatus::Wanted);
        }
        active.update(&state.db).await?;
    }

    for path in prunable_dirs {
        prune_empty_parent_dirs(&path, &state.music_root).await?;
    }

    Ok(removed_any)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sea_orm::{
        ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait,
        IntoActiveModel, QueryFilter,
    };
    use yoink_shared::DownloadJobKind;

    use super::{
        cancel_job, clear_completed_jobs, enqueue_album_download, enqueue_track_download,
        list_jobs, retry_album_download, sync_album_wanted_status_from_tracks,
    };
    use crate::{
        app_config::AuthConfig,
        db::{
            self, album, album_artist, album_type::AlbumType, artist, download_job,
            download_status::DownloadStatus, provider::Provider, quality::Quality, track,
            wanted_status::WantedStatus,
        },
        error::AppError,
        providers::registry::ProviderRegistry,
        state::AppState,
    };

    async fn test_state() -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-downloads-test-{}.db?mode=rwc",
            uuid::Uuid::now_v7()
        );

        AppState::new(
            PathBuf::from("./music"),
            Quality::Lossless,
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

    async fn seed_album_with_tracks(
        state: &AppState,
    ) -> (artist::Model, album::Model, track::Model, track::Model) {
        let artist = artist::ActiveModel {
            name: Set("Test Artist".to_string()),
            image_url: Set(None),
            bio: Set(None),
            monitored: Set(true),
            ..artist::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert artist");

        let album = album::ActiveModel {
            title: Set("Test Album".to_string()),
            album_type: Set(AlbumType::Album),
            release_date: Set(None),
            cover_url: Set(None),
            explicit: Set(false),
            wanted_status: Set(WantedStatus::Wanted),
            requested_quality: Set(None),
            ..album::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album");

        album_artist::ActiveModel {
            album_id: Set(album.id),
            artist_id: Set(artist.id),
            priority: Set(0),
        }
        .insert(&state.db)
        .await
        .expect("insert album artist");

        let track_one = track::ActiveModel {
            title: Set("Track One".to_string()),
            version: Set(None),
            disc_number: Set(Some(1)),
            track_number: Set(Some(1)),
            duration: Set(Some(180)),
            album_id: Set(album.id),
            explicit: Set(false),
            isrc: Set(None),
            root_folder_id: Set(None),
            status: Set(WantedStatus::Wanted),
            file_path: Set(None),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert track one");

        let track_two = track::ActiveModel {
            title: Set("Track Two".to_string()),
            version: Set(None),
            disc_number: Set(Some(1)),
            track_number: Set(Some(2)),
            duration: Set(Some(200)),
            album_id: Set(album.id),
            explicit: Set(false),
            isrc: Set(None),
            root_folder_id: Set(None),
            status: Set(WantedStatus::Unmonitored),
            file_path: Set(None),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert track two");

        (artist, album, track_one, track_two)
    }

    #[tokio::test]
    async fn list_jobs_returns_album_and_track_jobs() {
        let state = test_state().await;
        let (_artist, album, track_one, _track_two) = seed_album_with_tracks(&state).await;

        download_job::ActiveModel {
            album_id: Set(album.id),
            track_id: Set(None),
            source: Set(Provider::Tidal),
            quality: Set(Quality::Lossless),
            status: Set(DownloadStatus::Queued),
            total_tracks: Set(2),
            completed_tasks: Set(0),
            error_message: Set(None),
            ..download_job::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album job");

        download_job::ActiveModel {
            album_id: Set(album.id),
            track_id: Set(Some(track_one.id)),
            source: Set(Provider::Tidal),
            quality: Set(Quality::Lossless),
            status: Set(DownloadStatus::Failed),
            total_tracks: Set(1),
            completed_tasks: Set(0),
            error_message: Set(Some("boom".to_string())),
            ..download_job::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert track job");

        let jobs = list_jobs(&state).await.expect("list jobs");

        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().any(|job| job.kind == DownloadJobKind::Album));

        let track_job = jobs
            .iter()
            .find(|job| job.kind == DownloadJobKind::Track)
            .expect("track job");

        assert_eq!(track_job.album_id, album.id);
        assert_eq!(track_job.track_id, Some(track_one.id));
        assert_eq!(track_job.track_title.as_deref(), Some("Track One"));
        assert_eq!(track_job.artist_name, "Test Artist");
        assert_eq!(track_job.error.as_deref(), Some("boom"));
    }

    #[tokio::test]
    async fn enqueue_track_download_rejects_when_album_job_is_active() {
        let state = test_state().await;
        let (_artist, album, track_one, _track_two) = seed_album_with_tracks(&state).await;

        enqueue_album_download(&state, album.id)
            .await
            .expect("enqueue album");

        let err = enqueue_track_download(&state, track_one.id)
            .await
            .expect_err("track job should conflict");

        assert!(matches!(err, AppError::Conflict { .. }));
    }

    #[tokio::test]
    async fn cancel_and_clear_jobs_operate_on_service_state() {
        let state = test_state().await;
        let (_artist, album, _track_one, _track_two) = seed_album_with_tracks(&state).await;

        enqueue_album_download(&state, album.id)
            .await
            .expect("enqueue album");

        let queued_job = db::download_job::Entity::find()
            .one(&state.db)
            .await
            .expect("load queued job")
            .expect("queued job exists");

        cancel_job(&state, queued_job.id)
            .await
            .expect("cancel queued job");

        assert!(
            db::download_job::Entity::find_by_id(queued_job.id)
                .one(&state.db)
                .await
                .expect("reload cancelled job")
                .is_none()
        );

        download_job::ActiveModel {
            album_id: Set(album.id),
            track_id: Set(None),
            source: Set(Provider::Tidal),
            quality: Set(Quality::Lossless),
            status: Set(DownloadStatus::Completed),
            total_tracks: Set(2),
            completed_tasks: Set(2),
            error_message: Set(None),
            ..download_job::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert completed job");

        download_job::ActiveModel {
            album_id: Set(album.id),
            track_id: Set(None),
            source: Set(Provider::Tidal),
            quality: Set(Quality::Lossless),
            status: Set(DownloadStatus::Failed),
            total_tracks: Set(2),
            completed_tasks: Set(1),
            error_message: Set(Some("failed".to_string())),
            ..download_job::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert failed job");

        clear_completed_jobs(&state)
            .await
            .expect("clear completed jobs");

        assert_eq!(
            db::download_job::Entity::find()
                .all(&state.db)
                .await
                .expect("reload jobs")
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn retry_album_download_creates_a_new_queued_job() {
        let state = test_state().await;
        let (_artist, album, _track_one, _track_two) = seed_album_with_tracks(&state).await;

        download_job::ActiveModel {
            album_id: Set(album.id),
            track_id: Set(None),
            source: Set(Provider::Tidal),
            quality: Set(Quality::Lossless),
            status: Set(DownloadStatus::Failed),
            total_tracks: Set(2),
            completed_tasks: Set(1),
            error_message: Set(Some("failed".to_string())),
            ..download_job::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert failed job");

        retry_album_download(&state, album.id)
            .await
            .expect("retry album download");

        let jobs = db::download_job::Entity::find()
            .filter(download_job::Column::AlbumId.eq(album.id))
            .all(&state.db)
            .await
            .expect("load jobs");

        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().any(|job| job.status == DownloadStatus::Queued));
        assert!(jobs.iter().any(|job| job.status == DownloadStatus::Failed));
    }

    #[tokio::test]
    async fn sync_album_wanted_status_only_promotes_fully_monitored_albums() {
        let state = test_state().await;
        let (_artist, album, track_one, track_two) = seed_album_with_tracks(&state).await;

        sync_album_wanted_status_from_tracks(&state, album.id)
            .await
            .expect("sync status");

        let reloaded_album = db::album::Entity::find_by_id(album.id)
            .one(&state.db)
            .await
            .expect("reload album")
            .expect("album exists");
        assert_eq!(reloaded_album.wanted_status, WantedStatus::Wanted);

        for track_id in [track_one.id, track_two.id] {
            let track = db::track::Entity::find_by_id(track_id)
                .one(&state.db)
                .await
                .expect("load track")
                .expect("track exists");

            let mut active = track.into_active_model();
            active.status = Set(WantedStatus::Acquired);
            active.file_path = Set(Some("Artist/Album/01 - Track.flac".to_string()));
            active.update(&state.db).await.expect("mark track acquired");
        }

        sync_album_wanted_status_from_tracks(&state, album.id)
            .await
            .expect("sync acquired status");

        let reloaded_album = db::album::Entity::find_by_id(album.id)
            .one(&state.db)
            .await
            .expect("reload album")
            .expect("album exists");
        assert_eq!(reloaded_album.wanted_status, WantedStatus::Acquired);
    }
}
