use std::{collections::HashMap, path::Path};

use crate::{
    api::{
        ImportConfirmation, ImportMatchStatus, ImportPreviewItem, ImportResultSummary,
        ManualImportMode,
    },
    error::AppResult,
    state::AppState,
};

mod discover;
mod matcher;
mod persist;
mod types;

use discover::discover_albums;
use matcher::{
    build_candidates, is_album_already_imported, load_imported_paths, load_local_artist_catalog,
};
use types::MATCHED_CONFIDENCE;

pub(super) async fn preview_source(
    state: &AppState,
    root_path: &Path,
) -> AppResult<Vec<ImportPreviewItem>> {
    let discovered = discover_albums(root_path).await?;
    let catalog = load_local_artist_catalog(state).await?;
    let imported_paths = load_imported_paths(state, root_path).await?;

    let mut items = Vec::with_capacity(discovered.len());
    for album in discovered {
        let mut candidates = build_candidates(&album, &catalog);
        let already_imported = is_album_already_imported(&album, &imported_paths, root_path);
        let selected_candidate = candidates
            .first()
            .map(|_| 0usize)
            .filter(|_| !already_imported);
        let match_status = if already_imported {
            ImportMatchStatus::Matched
        } else if let Some(best) = candidates.first() {
            if best.album_id.is_some() && best.confidence >= MATCHED_CONFIDENCE {
                ImportMatchStatus::Matched
            } else {
                ImportMatchStatus::Partial
            }
        } else {
            ImportMatchStatus::Unmatched
        };

        items.push(ImportPreviewItem {
            id: album.id,
            relative_path: album.relative_path,
            discovered_artist: album.discovered_artist,
            discovered_album: album.discovered_album,
            discovered_year: album.discovered_year,
            match_status,
            selected_candidate,
            candidates: std::mem::take(&mut candidates),
            already_imported,
            audio_file_count: album.files.len(),
        });
    }

    Ok(items)
}

pub(super) async fn confirm_source(
    state: &AppState,
    root_path: &Path,
    external_mode: Option<ManualImportMode>,
    items: Vec<ImportConfirmation>,
) -> AppResult<ImportResultSummary> {
    let discovered = discover_albums(root_path).await?;
    let discovered_by_id: HashMap<String, types::DiscoveredAlbum> = discovered
        .into_iter()
        .map(|item| (item.id.clone(), item))
        .collect();

    let mut summary = ImportResultSummary {
        total_selected: items.len(),
        imported: 0,
        artists_added: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for confirmation in items {
        let Some(album) = discovered_by_id.get(&confirmation.preview_id) else {
            summary.failed += 1;
            summary.errors.push(format!(
                "Preview item `{}` is no longer available",
                confirmation.preview_id
            ));
            continue;
        };

        match persist::import_album_confirmation(
            state,
            root_path,
            external_mode,
            album,
            &confirmation,
        )
        .await
        {
            Ok(artists_added) => {
                summary.imported += 1;
                summary.artists_added += artists_added;
            }
            Err(err) => {
                summary.failed += 1;
                summary.errors.push(format!(
                    "{} / {}: {}",
                    confirmation.artist_name, confirmation.album_title, err
                ));
            }
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use chrono::NaiveDate;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    use tempfile::tempdir;

    use crate::{
        app_config::AuthConfig,
        db::{album, album_type::AlbumType, artist, track, wanted_status::WantedStatus},
        providers::registry::ProviderRegistry,
        state::AppState,
    };

    use super::{
        build_candidates, confirm_source,
        discover::summarize_discovered_album,
        preview_source,
        types::{DiscoveredAlbum, EmbeddedTrackMetadata, LocalArtistCatalog, ScannedAudioFile},
    };

    async fn test_state(music_root: PathBuf) -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-import-test-{}.db?mode=rwc",
            uuid::Uuid::now_v7()
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

    #[test]
    fn summarize_album_prefers_embedded_metadata_over_path() {
        let root = Path::new("/music");
        let album_dir = PathBuf::from("/music/Path Artist/Path Album (1999)");
        let files = vec![ScannedAudioFile {
            absolute_path: album_dir.join("01 - Path Track.mp3"),
            embedded: EmbeddedTrackMetadata {
                album_artist: Some("Tagged Artist".to_string()),
                album_title: Some("Tagged Album".to_string()),
                year: Some("2005".to_string()),
                ..EmbeddedTrackMetadata::default()
            },
        }];

        let discovered = summarize_discovered_album(root, album_dir, files);

        assert_eq!(discovered.discovered_artist, "Tagged Artist");
        assert_eq!(discovered.discovered_album, "Tagged Album");
        assert_eq!(discovered.discovered_year.as_deref(), Some("2005"));
    }

    #[test]
    fn build_candidates_rejects_unrelated_partial_match() {
        let now = chrono::Utc::now();
        let discovered = DiscoveredAlbum {
            id: "NotiON/FORWARDS (2024)".to_string(),
            relative_path: "NotiON/FORWARDS (2024)".to_string(),
            discovered_artist: "NOTION".to_string(),
            discovered_album: "FORWARDS".to_string(),
            discovered_year: Some("2024".to_string()),
            files: Vec::new(),
        };
        let catalog = vec![LocalArtistCatalog {
            artist: artist::Model {
                id: uuid::Uuid::now_v7(),
                name: "K Motionz".to_string(),
                image_url: None,
                bio: None,
                monitored: true,
                created_at: now,
                modified_at: now,
            },
            albums: vec![
                album::Model {
                    id: uuid::Uuid::now_v7(),
                    title: "For Old Times Sake EP".to_string(),
                    album_type: AlbumType::Unknown,
                    release_date: Some(NaiveDate::from_ymd_opt(2024, 1, 1).expect("valid date")),
                    cover_url: None,
                    explicit: false,
                    wanted_status: WantedStatus::Wanted,
                    requested_quality: None,
                    created_at: now,
                    modified_at: now,
                },
                album::Model {
                    id: uuid::Uuid::now_v7(),
                    title: "Trapline".to_string(),
                    album_type: AlbumType::Unknown,
                    release_date: Some(NaiveDate::from_ymd_opt(2024, 1, 1).expect("valid date")),
                    cover_url: None,
                    explicit: false,
                    wanted_status: WantedStatus::Wanted,
                    requested_quality: None,
                    created_at: now,
                    modified_at: now,
                },
            ],
        }];

        let candidates = build_candidates(&discovered, &catalog);

        assert!(candidates.is_empty());
    }

    #[tokio::test]
    async fn confirm_library_import_creates_artist_album_and_tracks() {
        let music_root = tempdir().expect("create music root");
        let artist_dir = music_root.path().join("Test Artist");
        let album_dir = artist_dir.join("Test Album (2023)");
        std::fs::create_dir_all(&album_dir).expect("create album dir");
        std::fs::write(album_dir.join("01 - First Song.mp3"), b"one").expect("write track");
        std::fs::write(album_dir.join("02 - Second Song.mp3"), b"two").expect("write track");

        let state = test_state(music_root.path().to_path_buf()).await;
        let preview = preview_source(&state, music_root.path())
            .await
            .expect("preview import");
        assert_eq!(preview.len(), 1);

        let summary = confirm_source(
            &state,
            music_root.path(),
            None,
            vec![crate::api::ImportConfirmation {
                preview_id: preview[0].id.clone(),
                artist_name: preview[0].discovered_artist.clone(),
                album_title: preview[0].discovered_album.clone(),
                year: preview[0].discovered_year.clone(),
                artist_id: None,
                album_id: None,
            }],
        )
        .await
        .expect("confirm import");

        assert_eq!(summary.imported, 1);
        assert_eq!(summary.artists_added, 1);

        let artist = artist::Entity::find()
            .one(&state.db)
            .await
            .expect("load artist")
            .expect("artist exists");
        let album = album::Entity::find()
            .one(&state.db)
            .await
            .expect("load album")
            .expect("album exists");
        let tracks = track::Entity::find()
            .filter(track::Column::AlbumId.eq(album.id))
            .all(&state.db)
            .await
            .expect("load tracks");

        assert_eq!(artist.name, "Test Artist");
        assert_eq!(album.title, "Test Album");
        assert_eq!(album.wanted_status, WantedStatus::Acquired);
        assert_eq!(tracks.len(), 2);
        assert!(tracks.iter().all(|track| {
            track
                .file_path
                .as_deref()
                .is_some_and(|path| path.starts_with("Test Artist/"))
        }));
    }

    #[tokio::test]
    async fn confirm_external_import_copies_files_into_managed_library() {
        let music_root = tempdir().expect("create music root");
        let source_root = tempdir().expect("create source root");
        let artist_dir = source_root.path().join("Source Artist");
        let album_dir = artist_dir.join("Source Album (2024)");
        std::fs::create_dir_all(&album_dir).expect("create source album dir");
        let source_track = album_dir.join("01 - Outside Song.mp3");
        std::fs::write(&source_track, b"outside").expect("write source track");

        let state = test_state(music_root.path().to_path_buf()).await;
        let preview = preview_source(&state, source_root.path())
            .await
            .expect("preview import");

        let summary = confirm_source(
            &state,
            source_root.path(),
            Some(crate::api::ManualImportMode::Copy),
            vec![crate::api::ImportConfirmation {
                preview_id: preview[0].id.clone(),
                artist_name: preview[0].discovered_artist.clone(),
                album_title: preview[0].discovered_album.clone(),
                year: preview[0].discovered_year.clone(),
                artist_id: None,
                album_id: None,
            }],
        )
        .await
        .expect("confirm external import");

        assert_eq!(summary.imported, 1);

        let imported_track = track::Entity::find()
            .one(&state.db)
            .await
            .expect("load track")
            .expect("track exists");
        let relative_path = imported_track.file_path.expect("managed path");
        let managed_file = music_root.path().join(&relative_path);

        assert!(
            managed_file.exists(),
            "expected copied file at {}",
            managed_file.display()
        );
        assert_eq!(
            std::fs::read(&managed_file).expect("read managed file"),
            b"outside"
        );
        assert!(
            std::fs::read(&source_track).is_ok(),
            "source file should remain"
        );
    }
}
