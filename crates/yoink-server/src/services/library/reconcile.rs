use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
    TransactionTrait,
};
use tokio::fs;
use tracing::{debug, info};

use crate::{
    db::{track, wanted_status::WantedStatus},
    error::{AppError, AppResult},
    services::downloads::sync_album_wanted_status_from_tracks,
    state::AppState,
};

/// Reconcile library files on disk with the database.
pub(crate) async fn reconcile_library_files(state: &AppState) -> AppResult<usize> {
    info!(
        music_root = %state.music_root.display(),
        "Starting repair-only library reconciliation"
    );

    let linked_tracks = track::Entity::find()
        .filter(track::Column::FilePath.is_not_null())
        .all(&state.db)
        .await?;

    let scanned_tracks = linked_tracks.len();
    let mut invalid_paths = 0usize;
    let mut missing_files = 0usize;
    let mut repaired_tracks = 0usize;
    let mut unchanged_tracks = 0usize;
    let mut affected_album_ids = HashSet::new();

    let tx = state.db.begin().await?;

    for linked_track in linked_tracks {
        let Some(stored_path) = linked_track.file_path.as_deref() else {
            continue;
        };

        let resolved_path = resolve_managed_track_path(&state.music_root, stored_path);
        let exists = match resolved_path.as_ref() {
            Some(path) => fs::try_exists(path).await.map_err(|err| {
                AppError::filesystem("check file exists", path.display().to_string(), err)
            })?,
            None => false,
        };

        let current_status = linked_track.status.clone();
        let mut next_file_path = linked_track.file_path.clone();
        let mut next_root_folder_id = linked_track.root_folder_id;
        let mut next_status = current_status.clone();

        if exists {
            if current_status != WantedStatus::Unmonitored
                && current_status != WantedStatus::Acquired
            {
                next_status = WantedStatus::Acquired;
            }
        } else {
            if resolved_path.is_none() {
                invalid_paths += 1;
            } else {
                missing_files += 1;
            }
            next_file_path = None;
            next_root_folder_id = None;
            next_status = next_track_status_after_missing_file(&linked_track);
        }

        let changed = next_file_path != linked_track.file_path
            || next_root_folder_id != linked_track.root_folder_id
            || next_status != current_status;

        if !changed {
            unchanged_tracks += 1;
            continue;
        }

        let track_id = linked_track.id;
        let album_id = linked_track.album_id;
        let mut active = linked_track.into_active_model();
        active.file_path = Set(next_file_path);
        active.root_folder_id = Set(next_root_folder_id);
        active.status = Set(next_status.clone());
        active.update(&tx).await?;

        repaired_tracks += 1;
        affected_album_ids.insert(album_id);

        debug!(
            %track_id,
            %album_id,
            file_exists = exists,
            ?next_status,
            "Reconciled linked track"
        );
    }

    tx.commit().await?;

    for album_id in &affected_album_ids {
        sync_album_wanted_status_from_tracks(state, *album_id).await?;
    }

    if repaired_tracks > 0 {
        state.notify_sse();
    }

    info!(
        scanned_tracks,
        repaired_tracks,
        unchanged_tracks,
        invalid_paths,
        missing_files,
        recomputed_albums = affected_album_ids.len(),
        "Completed library reconciliation"
    );

    Ok(repaired_tracks)
}

fn next_track_status_after_missing_file(track: &track::Model) -> WantedStatus {
    match track.status {
        WantedStatus::Acquired => WantedStatus::Wanted,
        WantedStatus::Unmonitored => WantedStatus::Unmonitored,
        WantedStatus::Wanted => WantedStatus::Wanted,
        WantedStatus::InProgress => WantedStatus::InProgress,
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sea_orm::{EntityTrait, QueryFilter};
    use tempfile::tempdir;
    use tokio::sync::broadcast::error::{RecvError, TryRecvError};
    use uuid::Uuid;

    use crate::{
        app_config::AuthConfig,
        db::{album, album_artist, album_type::AlbumType, artist, root_folder},
        providers::registry::ProviderRegistry,
    };

    use super::*;

    async fn test_state(music_root: PathBuf) -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-reconcile-test-{}.db?mode=rwc",
            Uuid::now_v7()
        );

        AppState::new(
            music_root,
            crate::db::quality::Quality::Lossless,
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

    async fn seed_album(
        state: &AppState,
        wanted_status: WantedStatus,
    ) -> (artist::Model, album::Model) {
        let artist = artist::ActiveModel {
            name: Set("Test Artist".to_string()),
            image_url: Set(None),
            bio: Set(None),
            monitored: Set(true),
            ..Default::default()
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
            wanted_status: Set(wanted_status),
            requested_quality: Set(None),
            ..Default::default()
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

        (artist, album)
    }

    async fn seed_root_folder(state: &AppState, path: &Path) -> root_folder::Model {
        root_folder::ActiveModel {
            path: Set(path.display().to_string()),
            ..Default::default()
        }
        .insert(&state.db)
        .await
        .expect("insert root folder")
    }

    async fn seed_track(
        state: &AppState,
        album_id: Uuid,
        status: WantedStatus,
        file_path: Option<String>,
        root_folder_id: Option<Uuid>,
    ) -> track::Model {
        track::ActiveModel {
            title: Set("Track 1".to_string()),
            version: Set(None),
            disc_number: Set(Some(1)),
            track_number: Set(Some(1)),
            duration: Set(Some(180)),
            album_id: Set(album_id),
            explicit: Set(false),
            isrc: Set(None),
            root_folder_id: Set(root_folder_id),
            status: Set(status),
            file_path: Set(file_path),
            ..Default::default()
        }
        .insert(&state.db)
        .await
        .expect("insert track")
    }

    async fn load_track(state: &AppState, track_id: Uuid) -> track::Model {
        track::Entity::find_by_id(track_id)
            .one(&state.db)
            .await
            .expect("load track")
            .expect("track exists")
    }

    async fn load_album(state: &AppState, album_id: Uuid) -> album::Model {
        album::Entity::find_by_id(album_id)
            .one(&state.db)
            .await
            .expect("load album")
            .expect("album exists")
    }

    #[tokio::test]
    async fn reconcile_keeps_existing_acquired_file_attached() {
        let music_root = tempdir().expect("create music root");
        let file_path = "Test Artist/Test Album (2024)/01 - Track 1.mp3";
        let absolute_path = music_root.path().join(file_path);
        std::fs::create_dir_all(absolute_path.parent().expect("parent")).expect("create parent");
        std::fs::write(&absolute_path, b"audio").expect("write audio");

        let state = test_state(music_root.path().to_path_buf()).await;
        let (_artist, album) = seed_album(&state, WantedStatus::Acquired).await;
        let track = seed_track(
            &state,
            album.id,
            WantedStatus::Acquired,
            Some(file_path.to_string()),
            None,
        )
        .await;

        let repaired = reconcile_library_files(&state).await.expect("reconcile");
        let reloaded_track = load_track(&state, track.id).await;

        assert_eq!(repaired, 0);
        assert_eq!(reloaded_track.file_path.as_deref(), Some(file_path));
        assert_eq!(reloaded_track.status, WantedStatus::Acquired);
    }

    #[tokio::test]
    async fn reconcile_clears_missing_acquired_file() {
        let music_root = tempdir().expect("create music root");
        let state = test_state(music_root.path().to_path_buf()).await;
        let (_artist, album) = seed_album(&state, WantedStatus::Acquired).await;
        let root_folder = seed_root_folder(&state, music_root.path()).await;
        let track = seed_track(
            &state,
            album.id,
            WantedStatus::Acquired,
            Some("Test Artist/Test Album (2024)/01 - Track 1.mp3".to_string()),
            Some(root_folder.id),
        )
        .await;

        let repaired = reconcile_library_files(&state).await.expect("reconcile");
        let reloaded_track = load_track(&state, track.id).await;
        let reloaded_album = load_album(&state, album.id).await;

        assert_eq!(repaired, 1);
        assert_eq!(reloaded_track.file_path, None);
        assert_eq!(reloaded_track.root_folder_id, None);
        assert_eq!(reloaded_track.status, WantedStatus::Wanted);
        assert_eq!(reloaded_album.wanted_status, WantedStatus::Wanted);
    }

    #[tokio::test]
    async fn reconcile_keeps_missing_unmonitored_track_unmonitored() {
        let music_root = tempdir().expect("create music root");
        let state = test_state(music_root.path().to_path_buf()).await;
        let (_artist, album) = seed_album(&state, WantedStatus::Unmonitored).await;
        let track = seed_track(
            &state,
            album.id,
            WantedStatus::Unmonitored,
            Some("Test Artist/Test Album (2024)/01 - Track 1.mp3".to_string()),
            None,
        )
        .await;

        let repaired = reconcile_library_files(&state).await.expect("reconcile");
        let reloaded_track = load_track(&state, track.id).await;
        let reloaded_album = load_album(&state, album.id).await;

        assert_eq!(repaired, 1);
        assert_eq!(reloaded_track.file_path, None);
        assert_eq!(reloaded_track.status, WantedStatus::Unmonitored);
        assert_eq!(reloaded_album.wanted_status, WantedStatus::Unmonitored);
    }

    #[tokio::test]
    async fn reconcile_promotes_existing_monitored_file_to_acquired() {
        let music_root = tempdir().expect("create music root");
        let file_path = "Test Artist/Test Album (2024)/01 - Track 1.mp3";
        let absolute_path = music_root.path().join(file_path);
        std::fs::create_dir_all(absolute_path.parent().expect("parent")).expect("create parent");
        std::fs::write(&absolute_path, b"audio").expect("write audio");

        let state = test_state(music_root.path().to_path_buf()).await;
        let (_artist, album) = seed_album(&state, WantedStatus::Wanted).await;
        let track = seed_track(
            &state,
            album.id,
            WantedStatus::Wanted,
            Some(file_path.to_string()),
            None,
        )
        .await;

        let repaired = reconcile_library_files(&state).await.expect("reconcile");
        let reloaded_track = load_track(&state, track.id).await;
        let reloaded_album = load_album(&state, album.id).await;

        assert_eq!(repaired, 1);
        assert_eq!(reloaded_track.status, WantedStatus::Acquired);
        assert_eq!(reloaded_album.wanted_status, WantedStatus::Acquired);
    }

    #[tokio::test]
    async fn reconcile_treats_parent_dir_path_as_missing() {
        let music_root = tempdir().expect("create music root");
        let state = test_state(music_root.path().to_path_buf()).await;
        let (_artist, album) = seed_album(&state, WantedStatus::Acquired).await;
        let track = seed_track(
            &state,
            album.id,
            WantedStatus::Acquired,
            Some("../outside.mp3".to_string()),
            None,
        )
        .await;

        let repaired = reconcile_library_files(&state).await.expect("reconcile");
        let reloaded_track = load_track(&state, track.id).await;

        assert_eq!(repaired, 1);
        assert_eq!(reloaded_track.file_path, None);
        assert_eq!(reloaded_track.status, WantedStatus::Wanted);
    }

    #[tokio::test]
    async fn reconcile_treats_absolute_path_outside_root_as_missing() {
        let music_root = tempdir().expect("create music root");
        let outside_root = tempdir().expect("create outside root");
        let outside_path = outside_root.path().join("outside.mp3");
        std::fs::write(&outside_path, b"audio").expect("write audio");

        let state = test_state(music_root.path().to_path_buf()).await;
        let (_artist, album) = seed_album(&state, WantedStatus::Acquired).await;
        let track = seed_track(
            &state,
            album.id,
            WantedStatus::Acquired,
            Some(outside_path.display().to_string()),
            None,
        )
        .await;

        let repaired = reconcile_library_files(&state).await.expect("reconcile");
        let reloaded_track = load_track(&state, track.id).await;

        assert_eq!(repaired, 1);
        assert_eq!(reloaded_track.file_path, None);
        assert_eq!(reloaded_track.status, WantedStatus::Wanted);
    }

    #[tokio::test]
    async fn reconcile_is_idempotent_on_second_run() {
        let music_root = tempdir().expect("create music root");
        let state = test_state(music_root.path().to_path_buf()).await;
        let (_artist, album) = seed_album(&state, WantedStatus::Acquired).await;
        let track = seed_track(
            &state,
            album.id,
            WantedStatus::Acquired,
            Some("Test Artist/Test Album (2024)/01 - Track 1.mp3".to_string()),
            None,
        )
        .await;

        let repaired_first = reconcile_library_files(&state)
            .await
            .expect("first reconcile");
        let first_track = load_track(&state, track.id).await;
        let repaired_second = reconcile_library_files(&state)
            .await
            .expect("second reconcile");
        let second_track = load_track(&state, track.id).await;

        assert_eq!(repaired_first, 1);
        assert_eq!(repaired_second, 0);
        assert_eq!(first_track.file_path, None);
        assert_eq!(second_track.file_path, None);
        assert_eq!(second_track.status, WantedStatus::Wanted);
    }

    #[tokio::test]
    async fn reconcile_emits_sse_only_when_changes_occur() {
        let music_root = tempdir().expect("create music root");
        let file_path = "Test Artist/Test Album (2024)/01 - Track 1.mp3";
        let absolute_path = music_root.path().join(file_path);
        std::fs::create_dir_all(absolute_path.parent().expect("parent")).expect("create parent");
        std::fs::write(&absolute_path, b"audio").expect("write audio");

        let state = test_state(music_root.path().to_path_buf()).await;
        let mut rx = state.sse_tx.subscribe();
        let (_artist, album) = seed_album(&state, WantedStatus::Acquired).await;
        let _track = seed_track(
            &state,
            album.id,
            WantedStatus::Acquired,
            Some(file_path.to_string()),
            None,
        )
        .await;

        let repaired = reconcile_library_files(&state).await.expect("reconcile");
        assert_eq!(repaired, 0);
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

        std::fs::remove_file(&absolute_path).expect("remove audio");
        let repaired = reconcile_library_files(&state)
            .await
            .expect("reconcile after delete");
        assert_eq!(repaired, 1);

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("sse timeout");
        assert!(matches!(event, Result::<(), RecvError>::Ok(())));
    }

    #[tokio::test]
    async fn reconcile_does_not_fail_when_music_root_is_missing() {
        let missing_root =
            std::env::temp_dir().join(format!("yoink-missing-root-{}", Uuid::now_v7()));
        let state = test_state(missing_root).await;

        let repaired = reconcile_library_files(&state).await.expect("reconcile");

        assert_eq!(repaired, 0);
    }

    #[tokio::test]
    async fn reconcile_does_not_delete_tracks_for_missing_files() {
        let music_root = tempdir().expect("create music root");
        let state = test_state(music_root.path().to_path_buf()).await;
        let (_artist, album) = seed_album(&state, WantedStatus::Acquired).await;
        let track = seed_track(
            &state,
            album.id,
            WantedStatus::Acquired,
            Some("Test Artist/Test Album (2024)/01 - Track 1.mp3".to_string()),
            None,
        )
        .await;

        reconcile_library_files(&state).await.expect("reconcile");

        let persisted = track::Entity::find()
            .filter(track::Column::AlbumId.eq(album.id))
            .all(&state.db)
            .await
            .expect("load tracks");

        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].id, track.id);
    }
}
