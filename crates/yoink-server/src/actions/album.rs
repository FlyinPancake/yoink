use chrono::Utc;
use tracing::info;
use uuid::Uuid;
use yoink_shared::Quality;

use crate::{
    db,
    error::{AppError, AppResult},
    models::DownloadStatus,
    services,
    state::AppState,
};

use super::helpers;

pub(super) async fn toggle_album_monitor(
    state: &AppState,
    album_id: Uuid,
    monitored: bool,
) -> AppResult<()> {
    let mut album_to_queue = None;
    {
        let mut albums = state.monitored_albums.write().await;
        if let Some(album) = albums.iter_mut().find(|a| a.id == album_id) {
            album.monitored = monitored;
            services::update_wanted(album);
            db::update_album_flags(
                &state.db,
                album.id,
                album.monitored,
                album.acquired,
                album.wanted,
            )
            .await?;

            // Cascade the monitored state to all tracks on this album.
            db::bulk_update_track_monitored_for_album(&state.db, album_id, monitored).await?;
            services::recompute_partially_wanted(&state.db, album).await;

            if album.monitored && !album.acquired {
                album_to_queue = Some(album.clone());
            }
        }
    }
    if let Some(album) = album_to_queue {
        services::enqueue_album_download(state, &album).await;
    }
    state.notify_sse();
    Ok(())
}

pub(super) async fn bulk_monitor(
    state: &AppState,
    artist_id: Uuid,
    monitored: bool,
) -> AppResult<()> {
    let mut to_queue = Vec::new();
    {
        let mut albums = state.monitored_albums.write().await;
        for album in albums
            .iter_mut()
            .filter(|a| a.artist_id == artist_id || a.artist_ids.contains(&artist_id))
        {
            album.monitored = monitored;
            services::update_wanted(album);
            db::update_album_flags(
                &state.db,
                album.id,
                album.monitored,
                album.acquired,
                album.wanted,
            )
            .await?;

            // Cascade the monitored state to all tracks on this album.
            db::bulk_update_track_monitored_for_album(&state.db, album.id, monitored).await?;
            services::recompute_partially_wanted(&state.db, album).await;

            if album.monitored && !album.acquired {
                to_queue.push(album.clone());
            }
        }
    }
    for album in to_queue {
        services::enqueue_album_download(state, &album).await;
    }
    state.notify_sse();
    Ok(())
}

pub(super) async fn set_album_quality(
    state: &AppState,
    album_id: Uuid,
    quality: Option<Quality>,
) -> AppResult<()> {
    db::update_album_quality_override(&state.db, album_id, quality).await?;

    {
        let mut albums = state.monitored_albums.write().await;
        let album = albums
            .iter_mut()
            .find(|a| a.id == album_id)
            .ok_or_else(|| AppError::not_found("album", Some(album_id.to_string())))?;
        album.quality_override = quality;
    }

    let effective_quality = quality.unwrap_or(state.default_quality);
    {
        let mut jobs = state.download_jobs.write().await;
        for job in jobs.iter_mut().filter(|j| {
            j.album_id == album_id
                && matches!(j.status, DownloadStatus::Queued | DownloadStatus::Failed)
        }) {
            job.quality = effective_quality;
            job.updated_at = Utc::now();
            db::update_job(&state.db, job).await?;
        }
    }

    info!(%album_id, quality = ?quality, "Updated album quality override");
    state.notify_sse();
    Ok(())
}

pub(super) async fn merge_albums(
    state: &AppState,
    target_album_id: Uuid,
    source_album_id: Uuid,
    result_title: Option<String>,
    result_cover_url: Option<String>,
) -> AppResult<()> {
    services::merge_albums(
        state,
        target_album_id,
        source_album_id,
        result_title.as_deref(),
        result_cover_url.as_deref(),
    )
    .await?;

    let artist_id = {
        let albums = state.monitored_albums.read().await;
        albums
            .iter()
            .find(|a| a.id == target_album_id)
            .map(|a| a.artist_id)
    };
    if let Some(artist_id) = artist_id {
        helpers::spawn_recompute_artist_match_suggestions(state, artist_id);
    }
    state.notify_sse();
    Ok(())
}

pub(super) async fn remove_album_files(
    state: &AppState,
    album_id: Uuid,
    unmonitor: bool,
) -> AppResult<()> {
    let album = {
        let albums = state.monitored_albums.read().await;
        albums.iter().find(|a| a.id == album_id).cloned()
    }
    .ok_or_else(|| AppError::not_found("album", Some(album_id.to_string())))?;

    let removed = services::remove_downloaded_album_files(state, &album).await?;

    let mut to_queue = None;
    {
        let mut albums = state.monitored_albums.write().await;
        if let Some(existing) = albums.iter_mut().find(|a| a.id == album_id) {
            existing.acquired = false;
            if unmonitor {
                existing.monitored = false;
                // Cascade unmonitor to all tracks on this album.
                db::bulk_update_track_monitored_for_album(&state.db, album_id, false).await?;
            }
            services::update_wanted(existing);
            db::update_album_flags(
                &state.db,
                existing.id,
                existing.monitored,
                existing.acquired,
                existing.wanted,
            )
            .await?;
            services::recompute_partially_wanted(&state.db, existing).await;
            if existing.monitored {
                to_queue = Some(existing.clone());
            }
        }
    }

    let mut removed_completed_ids = Vec::new();
    {
        let mut jobs = state.download_jobs.write().await;
        jobs.retain(|j| {
            let should_remove =
                j.album_id == album_id && j.status == yoink_shared::DownloadStatus::Completed;
            if should_remove {
                removed_completed_ids.push(j.id);
            }
            !should_remove
        });
    }
    for job_id in removed_completed_ids {
        db::delete_job(&state.db, job_id).await?;
    }

    if let Some(album) = to_queue {
        services::enqueue_album_download(state, &album).await;
    }

    info!(
        %album_id,
        removed, unmonitor, "Removed downloaded album files"
    );
    state.notify_sse();
    Ok(())
}

pub(super) async fn add_album_artist(
    state: &AppState,
    album_id: Uuid,
    artist_id: Uuid,
) -> AppResult<()> {
    db::add_album_artist(&state.db, album_id, artist_id).await?;
    {
        let mut albums = state.monitored_albums.write().await;
        if let Some(album) = albums.iter_mut().find(|a| a.id == album_id)
            && !album.artist_ids.contains(&artist_id)
        {
            album.artist_ids.push(artist_id);
        }
    }
    state.notify_sse();
    Ok(())
}

pub(super) async fn remove_album_artist(
    state: &AppState,
    album_id: Uuid,
    artist_id: Uuid,
) -> AppResult<()> {
    // Must keep at least one artist
    {
        let albums = state.monitored_albums.read().await;
        if let Some(album) = albums.iter().find(|a| a.id == album_id)
            && album.artist_ids.len() <= 1
        {
            return Err(AppError::conflict(
                "cannot remove the only artist from an album",
            ));
        }
    }
    db::remove_album_artist(&state.db, album_id, artist_id).await?;
    {
        let mut albums = state.monitored_albums.write().await;
        if let Some(album) = albums.iter_mut().find(|a| a.id == album_id) {
            album.artist_ids.retain(|id| *id != artist_id);
            if album.artist_id == artist_id && !album.artist_ids.is_empty() {
                album.artist_id = album.artist_ids[0];
                // Update the legacy column
                db::upsert_album(&state.db, album).await?;
            }
        }
    }
    state.notify_sse();
    Ok(())
}

pub(super) async fn add_album(
    state: &AppState,
    provider: String,
    external_album_id: String,
    artist_external_id: String,
    artist_name: String,
    monitor_all: bool,
) -> AppResult<()> {
    // 1. Find or create lightweight (unmonitored) artist.
    let artist_id = helpers::find_or_create_lightweight_artist(
        state,
        &provider,
        &artist_external_id,
        &artist_name,
    )
    .await?;

    // 2. Fetch album metadata from the provider.
    let prov = state.registry.metadata_provider(&provider).ok_or_else(|| {
        AppError::unavailable(
            "metadata provider",
            format!("unknown provider '{provider}'"),
        )
    })?;

    let albums = prov.fetch_albums(&artist_external_id).await?;

    let prov_album = albums
        .into_iter()
        .find(|a| a.external_id == external_album_id)
        .ok_or_else(|| {
            AppError::not_found(
                "provider album",
                Some(format!("{provider}:{external_album_id}")),
            )
        })?;

    // 3. Check if album already exists via provider link.
    let existing_album_id =
        db::find_album_by_provider_link(&state.db, &provider, &external_album_id)
            .await
            .ok()
            .flatten();

    let album_id = if let Some(id) = existing_album_id {
        id
    } else {
        let new_id = Uuid::now_v7();
        let album = yoink_shared::MonitoredAlbum {
            id: new_id,
            artist_id,
            artist_ids: vec![artist_id],
            artist_credits: prov_album
                .artists
                .iter()
                .map(|a| yoink_shared::ArtistCredit {
                    name: a.name.clone(),
                    provider: Some(provider.clone()),
                    external_id: Some(a.external_id.clone()),
                })
                .collect(),
            title: prov_album.title.clone(),
            album_type: prov_album.album_type.clone(),
            release_date: prov_album.release_date.clone(),
            cover_url: prov_album
                .cover_ref
                .as_deref()
                .map(|c| yoink_shared::provider_image_url(&provider, c, 640)),
            explicit: prov_album.explicit,
            quality_override: None,
            monitored: monitor_all,
            acquired: false,
            wanted: monitor_all,
            partially_wanted: false,
            added_at: Utc::now(),
        };
        db::upsert_album(&state.db, &album).await?;

        let link = db::AlbumProviderLink {
            id: Uuid::now_v7(),
            album_id: new_id,
            provider: provider.clone(),
            external_id: external_album_id.clone(),
            external_url: prov_album.url.clone(),
            external_title: Some(prov_album.title.clone()),
            cover_ref: prov_album.cover_ref.clone(),
        };
        db::upsert_album_provider_link(&state.db, &link).await?;
        db::add_album_artist(&state.db, new_id, artist_id).await?;

        {
            let mut albums = state.monitored_albums.write().await;
            albums.push(album);
        }
        new_id
    };

    // 4. Fetch and store tracks.
    helpers::store_album_tracks(state, &provider, &external_album_id, album_id, monitor_all)
        .await?;

    // 5. If monitored, queue download.
    if monitor_all {
        let album = {
            let albums = state.monitored_albums.read().await;
            albums.iter().find(|a| a.id == album_id).cloned()
        };
        if let Some(album) = album {
            services::enqueue_album_download(state, &album).await;
        }
    }

    info!(%album_id, %provider, %external_album_id, monitor_all, "Added album from search");
    state.notify_sse();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::db;
    use crate::providers::registry::ProviderRegistry;
    use crate::providers::{ProviderAlbum, ProviderAlbumArtist, ProviderTrack};
    use crate::test_helpers::*;
    use chrono::Utc;
    use yoink_shared::Quality;

    // ── ToggleAlbumMonitor ──────────────────────────────────────

    #[tokio::test]
    async fn toggle_album_monitor_sets_flags() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let album = seed_album(&state.db, artist.id, "Album").await;

        state.monitored_artists.write().await.push(artist.clone());
        state.monitored_albums.write().await.push(album.clone());

        super::toggle_album_monitor(&state, album.id, false)
            .await
            .unwrap();

        let albums = state.monitored_albums.read().await;
        let a = albums.iter().find(|a| a.id == album.id).unwrap();
        assert!(!a.monitored);
        assert!(!a.wanted);
    }

    #[tokio::test]
    async fn toggle_album_monitor_enqueues_download_when_monitored() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let mut album = seed_album(&state.db, artist.id, "Album").await;
        album.monitored = false;
        album.wanted = false;
        album.acquired = false;
        db::upsert_album(&state.db, &album).await.unwrap();

        state.monitored_artists.write().await.push(artist.clone());
        state.monitored_albums.write().await.push(album.clone());

        super::toggle_album_monitor(&state, album.id, true)
            .await
            .unwrap();

        let albums = state.monitored_albums.read().await;
        let a = albums.iter().find(|a| a.id == album.id).unwrap();
        assert!(a.monitored);

        let jobs = state.download_jobs.read().await;
        assert!(
            jobs.iter().any(|j| j.album_id == album.id),
            "should have enqueued a download job"
        );
    }

    #[tokio::test]
    async fn toggle_album_monitor_cascades_to_tracks() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let album = seed_album(&state.db, artist.id, "Album").await;
        seed_tracks(&state.db, album.id, 3).await;

        state.monitored_artists.write().await.push(artist.clone());
        state.monitored_albums.write().await.push(album.clone());

        // Monitor the album — tracks should become monitored
        super::toggle_album_monitor(&state, album.id, true)
            .await
            .unwrap();

        let loaded = db::load_tracks_for_album(&state.db, album.id)
            .await
            .unwrap();
        assert!(
            loaded.iter().all(|t| t.monitored),
            "all tracks should be monitored after monitoring album"
        );

        // Unmonitor the album — tracks should become unmonitored
        super::toggle_album_monitor(&state, album.id, false)
            .await
            .unwrap();

        let loaded = db::load_tracks_for_album(&state.db, album.id)
            .await
            .unwrap();
        assert!(
            loaded.iter().all(|t| !t.monitored),
            "all tracks should be unmonitored after unmonitoring album"
        );

        // partially_wanted should be false since no tracks are monitored
        let albums = state.monitored_albums.read().await;
        let a = albums.iter().find(|a| a.id == album.id).unwrap();
        assert!(!a.partially_wanted);
    }

    // ── BulkMonitor ─────────────────────────────────────────────

    #[tokio::test]
    async fn bulk_monitor_toggles_all_albums_for_artist() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let a1 = seed_album(&state.db, artist.id, "Album 1").await;
        let a2 = seed_album(&state.db, artist.id, "Album 2").await;

        state.monitored_artists.write().await.push(artist.clone());
        {
            let mut albums = state.monitored_albums.write().await;
            albums.push(a1.clone());
            albums.push(a2.clone());
        }

        super::bulk_monitor(&state, artist.id, false).await.unwrap();

        let albums = state.monitored_albums.read().await;
        assert!(albums.iter().all(|a| !a.monitored));
    }

    #[tokio::test]
    async fn bulk_monitor_cascades_to_tracks() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let a1 = seed_album(&state.db, artist.id, "Album 1").await;
        let a2 = seed_album(&state.db, artist.id, "Album 2").await;
        seed_tracks(&state.db, a1.id, 2).await;
        seed_tracks(&state.db, a2.id, 3).await;

        state.monitored_artists.write().await.push(artist.clone());
        {
            let mut albums = state.monitored_albums.write().await;
            albums.push(a1.clone());
            albums.push(a2.clone());
        }

        // Monitor all albums — tracks should cascade
        super::bulk_monitor(&state, artist.id, true).await.unwrap();

        for album_id in [a1.id, a2.id] {
            let tracks = db::load_tracks_for_album(&state.db, album_id)
                .await
                .unwrap();
            assert!(
                tracks.iter().all(|t| t.monitored),
                "all tracks should be monitored after bulk monitor"
            );
        }

        // Unmonitor all albums — tracks should cascade
        super::bulk_monitor(&state, artist.id, false).await.unwrap();

        for album_id in [a1.id, a2.id] {
            let tracks = db::load_tracks_for_album(&state.db, album_id)
                .await
                .unwrap();
            assert!(
                tracks.iter().all(|t| !t.monitored),
                "all tracks should be unmonitored after bulk unmonitor"
            );
        }
    }

    #[tokio::test]
    async fn set_album_quality_updates_album_and_queued_failed_jobs_only() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let album = seed_album(&state.db, artist.id, "Album").await;

        state.monitored_artists.write().await.push(artist);
        state.monitored_albums.write().await.push(album.clone());

        for status in [
            crate::models::DownloadStatus::Queued,
            crate::models::DownloadStatus::Failed,
            crate::models::DownloadStatus::Resolving,
            crate::models::DownloadStatus::Downloading,
            crate::models::DownloadStatus::Completed,
        ] {
            let mut job = seed_job(&state.db, album.id, status.clone()).await;
            job.status = status;
            job.updated_at = Utc::now();
            db::update_job(&state.db, &job).await.unwrap();
            state.download_jobs.write().await.push(job);
        }

        super::set_album_quality(&state, album.id, Some(Quality::HiRes))
            .await
            .unwrap();

        let albums = state.monitored_albums.read().await;
        assert_eq!(albums[0].quality_override, Some(Quality::HiRes));
        drop(albums);

        let jobs = state.download_jobs.read().await;
        for job in jobs.iter() {
            match job.status {
                crate::models::DownloadStatus::Queued | crate::models::DownloadStatus::Failed => {
                    assert_eq!(job.quality, Quality::HiRes);
                }
                crate::models::DownloadStatus::Resolving
                | crate::models::DownloadStatus::Downloading
                | crate::models::DownloadStatus::Completed => {
                    assert_eq!(job.quality, Quality::High);
                }
            }
        }
    }

    // ── AddAlbumArtist / RemoveAlbumArtist ──────────────────────

    #[tokio::test]
    async fn add_and_remove_album_artist() {
        let (state, _tmp) = test_app_state().await;
        let a1 = seed_artist(&state.db, "Artist 1").await;
        let a2 = seed_artist(&state.db, "Artist 2").await;
        let album = seed_album(&state.db, a1.id, "Album").await;

        state.monitored_artists.write().await.push(a1.clone());
        state.monitored_artists.write().await.push(a2.clone());
        state.monitored_albums.write().await.push(album.clone());

        super::add_album_artist(&state, album.id, a2.id)
            .await
            .unwrap();

        {
            let albums = state.monitored_albums.read().await;
            let a = albums.iter().find(|a| a.id == album.id).unwrap();
            assert_eq!(a.artist_ids.len(), 2);
            assert!(a.artist_ids.contains(&a2.id));
        }

        super::remove_album_artist(&state, album.id, a1.id)
            .await
            .unwrap();

        {
            let albums = state.monitored_albums.read().await;
            let a = albums.iter().find(|a| a.id == album.id).unwrap();
            assert_eq!(a.artist_ids, vec![a2.id]);
            assert_eq!(a.artist_id, a2.id);
        }
    }

    #[tokio::test]
    async fn remove_sole_album_artist_returns_error() {
        let (state, _tmp) = test_app_state().await;
        let a1 = seed_artist(&state.db, "Solo").await;
        let album = seed_album(&state.db, a1.id, "Album").await;

        state.monitored_albums.write().await.push(album.clone());

        let result = super::remove_album_artist(&state, album.id, a1.id).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot remove the only artist")
        );
    }

    // ── MergeAlbums ─────────────────────────────────────────────

    #[tokio::test]
    async fn merge_albums_combines_tracks_and_links() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let target = seed_album(&state.db, artist.id, "Target Album").await;
        let source = seed_album(&state.db, artist.id, "Source Album").await;
        seed_tracks(&state.db, target.id, 2).await;
        seed_tracks(&state.db, source.id, 3).await;
        seed_album_provider_link(&state.db, source.id, "tidal", "SRC1").await;

        state.monitored_artists.write().await.push(artist.clone());
        {
            let mut albums = state.monitored_albums.write().await;
            albums.push(target.clone());
            albums.push(source.clone());
        }

        super::merge_albums(
            &state,
            target.id,
            source.id,
            Some("Merged Album".to_string()),
            None,
        )
        .await
        .unwrap();

        let albums = state.monitored_albums.read().await;
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].id, target.id);
        assert_eq!(albums[0].title, "Merged Album");

        let tracks = db::load_tracks_for_album(&state.db, target.id)
            .await
            .unwrap();
        assert_eq!(tracks.len(), 5);

        let links = db::load_album_provider_links(&state.db, target.id)
            .await
            .unwrap();
        assert!(links.iter().any(|l| l.external_id == "SRC1"));
    }

    #[tokio::test]
    async fn merge_albums_same_id_returns_error() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let album = seed_album(&state.db, artist.id, "Album").await;

        state.monitored_albums.write().await.push(album.clone());

        let result = super::merge_albums(&state, album.id, album.id, None, None).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must be different")
        );
    }

    // ── AddAlbum with mock provider ─────────────────────────────

    #[tokio::test]
    async fn add_album_creates_album_and_tracks() {
        let mock = Arc::new(MockMetadataProvider::new("mock_prov"));
        *mock.fetch_albums_result.lock().await = Ok(vec![ProviderAlbum {
            external_id: "ALB_EXT".to_string(),
            title: "Mock Album".to_string(),
            album_type: Some("album".to_string()),
            release_date: Some("2024-06-01".to_string()),
            cover_ref: None,
            url: None,
            explicit: false,
            artists: vec![ProviderAlbumArtist {
                external_id: "ART_EXT".to_string(),
                name: "Mock Artist".to_string(),
            }],
        }]);
        *mock.fetch_tracks_result.lock().await = Ok((
            vec![
                ProviderTrack {
                    external_id: "TRK1".to_string(),
                    title: "Track One".to_string(),
                    version: None,
                    track_number: 1,
                    disc_number: Some(1),
                    duration_secs: 200,
                    isrc: Some("US1234567890".to_string()),
                    artists: None,
                    explicit: false,
                    extra: std::collections::HashMap::new(),
                },
                ProviderTrack {
                    external_id: "TRK2".to_string(),
                    title: "Track Two".to_string(),
                    version: None,
                    track_number: 2,
                    disc_number: Some(1),
                    duration_secs: 180,
                    isrc: None,
                    artists: None,
                    explicit: false,
                    extra: std::collections::HashMap::new(),
                },
            ],
            std::collections::HashMap::new(),
        ));

        let mut registry = ProviderRegistry::new();
        registry.register_metadata(mock as Arc<dyn crate::providers::MetadataProvider>);

        let (state, _tmp) = test_app_state_with_registry(registry).await;

        super::add_album(
            &state,
            "mock_prov".to_string(),
            "ALB_EXT".to_string(),
            "ART_EXT".to_string(),
            "Mock Artist".to_string(),
            true,
        )
        .await
        .unwrap();

        let artists = state.monitored_artists.read().await;
        assert_eq!(artists.len(), 1);
        assert!(!artists[0].monitored); // lightweight

        let albums = state.monitored_albums.read().await;
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].title, "Mock Album");
        assert!(albums[0].monitored);
        let album_id = albums[0].id;
        drop(albums);

        let tracks = db::load_tracks_for_album(&state.db, album_id)
            .await
            .unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].title, "Track One");
        assert!(tracks[0].monitored);

        let album_links = db::load_album_provider_links(&state.db, album_id)
            .await
            .unwrap();
        assert_eq!(album_links.len(), 1);
        assert_eq!(album_links[0].external_id, "ALB_EXT");
    }
}
