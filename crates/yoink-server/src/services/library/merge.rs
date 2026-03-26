use uuid::Uuid;

use crate::{
    db::{self, album_type::AlbumType, quality::Quality, wanted_status::WantedStatus},
    error::{AppError, AppResult},
    state::AppState,
    util::normalize,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, TransactionTrait,
};

/// Merge a source album into a target album.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn merge_albums(
    state: &AppState,
    target_album_id: Uuid,
    source_album_id: Uuid,
    result_title: Option<&str>,
    result_cover_url: Option<&str>,
) -> AppResult<()> {
    if target_album_id == source_album_id {
        return Err(AppError::validation(
            Some("source_album_id"),
            "target and source album must be different",
        ));
    }

    let tx = state.db.begin().await?;

    let target_album = db::album::Entity::find_by_id(target_album_id)
        .one(&tx)
        .await?
        .ok_or_else(|| AppError::not_found("album", Some(target_album_id.to_string())))?;
    let source_album = db::album::Entity::find_by_id(source_album_id)
        .one(&tx)
        .await?
        .ok_or_else(|| AppError::not_found("album", Some(source_album_id.to_string())))?;

    let target_tracks = db::track::Entity::find()
        .filter(db::track::Column::AlbumId.eq(target_album_id))
        .all(&tx)
        .await?;
    let source_tracks = db::track::Entity::find()
        .filter(db::track::Column::AlbumId.eq(source_album_id))
        .all(&tx)
        .await?;

    let merged_album = merge_album_fields(
        target_album,
        &source_album,
        &target_tracks,
        &source_tracks,
        result_title,
        result_cover_url,
    );
    merged_album.update(&tx).await?;

    merge_album_artists(&tx, target_album_id, source_album_id).await?;
    merge_album_provider_links(&tx, target_album_id, source_album_id).await?;
    merge_tracks(&tx, target_album_id, source_tracks).await?;
    move_download_jobs(&tx, target_album_id, source_album_id).await?;

    db::album::Entity::delete_by_id(source_album_id)
        .exec(&tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

fn merge_album_fields(
    target_album: db::album::Model,
    source_album: &db::album::Model,
    target_tracks: &[db::track::Model],
    source_tracks: &[db::track::Model],
    result_title: Option<&str>,
    result_cover_url: Option<&str>,
) -> db::album::ActiveModel {
    let target_release_date = target_album.release_date;
    let target_cover_url = target_album.cover_url.clone();
    let target_title = target_album.title.clone();
    let target_album_type = target_album.album_type.clone();
    let target_explicit = target_album.explicit;
    let target_quality = target_album.requested_quality;

    let merged_status = combined_album_status(
        [
            target_album.wanted_status.clone(),
            source_album.wanted_status.clone(),
        ]
        .into_iter()
        .chain(target_tracks.iter().map(|track| track.status.clone()))
        .chain(source_tracks.iter().map(|track| track.status.clone())),
    );

    let merged_quality = prefer_album_quality(target_quality, source_album.requested_quality);

    let cover_url = match result_cover_url {
        Some(url) => Some(url.to_string()),
        None => target_cover_url.or_else(|| source_album.cover_url.clone()),
    };

    let title = result_title.map(ToOwned::to_owned).unwrap_or(target_title);

    let album_type = if target_album_type == AlbumType::Unknown {
        source_album.album_type.clone()
    } else {
        target_album_type
    };

    let mut merged = target_album.into_active_model();
    merged.title = Set(title);
    merged.album_type = Set(album_type);
    merged.release_date = Set(target_release_date.or(source_album.release_date));
    merged.cover_url = Set(cover_url);
    merged.explicit = Set(target_explicit || source_album.explicit);
    merged.wanted_status = Set(merged_status);
    merged.requested_quality = Set(merged_quality);
    merged
}

async fn merge_album_artists<C>(
    db: &C,
    target_album_id: Uuid,
    source_album_id: Uuid,
) -> AppResult<()>
where
    C: sea_orm::ConnectionTrait,
{
    let source_links = db::album_artist::Entity::find_by_album_ordered(source_album_id)
        .all(db)
        .await?;

    for source_link in source_links {
        let existing = db::album_artist::Entity::find()
            .filter(db::album_artist::Column::AlbumId.eq(target_album_id))
            .filter(db::album_artist::Column::ArtistId.eq(source_link.artist_id))
            .one(db)
            .await?;

        if let Some(existing) = existing {
            if source_link.priority < existing.priority {
                let mut existing = existing.into_active_model();
                existing.priority = Set(source_link.priority);
                existing.update(db).await?;
            }
        } else {
            db::album_artist::ActiveModel {
                album_id: Set(target_album_id),
                artist_id: Set(source_link.artist_id),
                priority: Set(source_link.priority),
            }
            .insert(db)
            .await?;
        }

        db::album_artist::Entity::delete_pair(source_album_id, source_link.artist_id)
            .exec(db)
            .await?;
    }

    Ok(())
}

async fn merge_album_provider_links<C>(
    db: &C,
    target_album_id: Uuid,
    source_album_id: Uuid,
) -> AppResult<()>
where
    C: sea_orm::ConnectionTrait,
{
    let source_links = db::album_provider_link::Entity::find()
        .filter(db::album_provider_link::Column::AlbumId.eq(source_album_id))
        .all(db)
        .await?;

    for source_link in source_links {
        let duplicate = db::album_provider_link::Entity::find()
            .filter(db::album_provider_link::Column::AlbumId.eq(target_album_id))
            .filter(db::album_provider_link::Column::Provider.eq(source_link.provider))
            .filter(
                db::album_provider_link::Column::ProviderAlbumId
                    .eq(source_link.provider_album_id.clone()),
            )
            .one(db)
            .await?;

        if duplicate.is_some() {
            db::album_provider_link::Entity::delete_by_id(source_link.id)
                .exec(db)
                .await?;
        } else {
            let mut source_link = source_link.into_active_model();
            source_link.album_id = Set(target_album_id);
            source_link.update(db).await?;
        }
    }

    Ok(())
}

async fn merge_tracks<C>(
    db: &C,
    target_album_id: Uuid,
    source_tracks: Vec<db::track::Model>,
) -> AppResult<()>
where
    C: sea_orm::ConnectionTrait,
{
    for source_track in source_tracks {
        let source_links = load_track_provider_links(db, source_track.id).await?;

        if let Some(target_track) =
            find_duplicate_target_track(db, target_album_id, &source_track, &source_links).await?
        {
            merge_track_artists(db, target_track.id, source_track.id).await?;
            merge_track_provider_links(db, target_track.id, source_track.id, &source_links).await?;

            let merged_track = merge_track_fields(target_track, &source_track);
            merged_track.update(db).await?;

            db::track::Entity::delete_by_id(source_track.id)
                .exec(db)
                .await?;
        } else {
            let mut source_track = source_track.into_active_model();
            source_track.album_id = Set(target_album_id);
            source_track.update(db).await?;
        }
    }

    Ok(())
}

async fn merge_track_artists<C>(
    db: &C,
    target_track_id: Uuid,
    source_track_id: Uuid,
) -> AppResult<()>
where
    C: sea_orm::ConnectionTrait,
{
    let source_artists = db::track_artist::Entity::find()
        .filter(db::track_artist::Column::TrackId.eq(source_track_id))
        .all(db)
        .await?;

    for source_artist in source_artists {
        let existing = db::track_artist::Entity::find()
            .filter(db::track_artist::Column::TrackId.eq(target_track_id))
            .filter(db::track_artist::Column::ArtistId.eq(source_artist.artist_id))
            .one(db)
            .await?;

        if let Some(existing) = existing {
            if source_artist.priority < existing.priority {
                let mut existing = existing.into_active_model();
                existing.priority = Set(source_artist.priority);
                existing.update(db).await?;
            }
        } else {
            db::track_artist::ActiveModel {
                track_id: Set(target_track_id),
                artist_id: Set(source_artist.artist_id),
                priority: Set(source_artist.priority),
            }
            .insert(db)
            .await?;
        }

        db::track_artist::Entity::delete_many()
            .filter(db::track_artist::Column::TrackId.eq(source_track_id))
            .filter(db::track_artist::Column::ArtistId.eq(source_artist.artist_id))
            .exec(db)
            .await?;
    }

    Ok(())
}

async fn merge_track_provider_links<C>(
    db: &C,
    target_track_id: Uuid,
    source_track_id: Uuid,
    source_links: &[db::track_provider_link::Model],
) -> AppResult<()>
where
    C: sea_orm::ConnectionTrait,
{
    for source_link in source_links {
        let duplicate = db::track_provider_link::Entity::find()
            .filter(db::track_provider_link::Column::TrackId.eq(target_track_id))
            .filter(db::track_provider_link::Column::Provider.eq(source_link.provider))
            .filter(
                db::track_provider_link::Column::ProviderTrackId
                    .eq(source_link.provider_track_id.clone()),
            )
            .one(db)
            .await?;

        if duplicate.is_some() {
            db::track_provider_link::Entity::delete_by_id(source_link.id)
                .exec(db)
                .await?;
        } else {
            let mut source_link = source_link.clone().into_active_model();
            source_link.track_id = Set(target_track_id);
            source_link.update(db).await?;
        }
    }

    db::track_provider_link::Entity::delete_many()
        .filter(db::track_provider_link::Column::TrackId.eq(source_track_id))
        .exec(db)
        .await?;

    Ok(())
}

fn merge_track_fields(
    target_track: db::track::Model,
    source_track: &db::track::Model,
) -> db::track::ActiveModel {
    let merged_version = target_track
        .version
        .clone()
        .or(source_track.version.clone());
    let merged_disc_number = target_track.disc_number.or(source_track.disc_number);
    let merged_track_number = target_track.track_number.or(source_track.track_number);
    let merged_duration = target_track.duration.or(source_track.duration);
    let merged_explicit = target_track.explicit || source_track.explicit;
    let merged_isrc = target_track.isrc.clone().or(source_track.isrc.clone());
    let merged_root_folder_id = target_track.root_folder_id.or(source_track.root_folder_id);
    let merged_status =
        prefer_track_status(target_track.status.clone(), source_track.status.clone());
    let merged_file_path = target_track
        .file_path
        .clone()
        .or(source_track.file_path.clone());

    let mut merged = target_track.into_active_model();
    merged.version = Set(merged_version);
    merged.disc_number = Set(merged_disc_number);
    merged.track_number = Set(merged_track_number);
    merged.duration = Set(merged_duration);
    merged.explicit = Set(merged_explicit);
    merged.isrc = Set(merged_isrc);
    merged.root_folder_id = Set(merged_root_folder_id);
    merged.status = Set(merged_status);
    merged.file_path = Set(merged_file_path);
    merged
}

async fn move_download_jobs<C>(
    db: &C,
    target_album_id: Uuid,
    source_album_id: Uuid,
) -> AppResult<()>
where
    C: sea_orm::ConnectionTrait,
{
    let source_jobs = db::download_job::Entity::find()
        .filter(db::download_job::Column::AlbumId.eq(source_album_id))
        .all(db)
        .await?;

    for source_job in source_jobs {
        let mut source_job = source_job.into_active_model();
        source_job.album_id = Set(target_album_id);
        source_job.update(db).await?;
    }

    Ok(())
}

async fn find_duplicate_target_track<C>(
    db: &C,
    target_album_id: Uuid,
    source_track: &db::track::Model,
    source_links: &[db::track_provider_link::Model],
) -> AppResult<Option<db::track::Model>>
where
    C: sea_orm::ConnectionTrait,
{
    let target_tracks = db::track::Entity::find()
        .filter(db::track::Column::AlbumId.eq(target_album_id))
        .order_by_asc(db::track::Column::DiscNumber)
        .order_by_asc(db::track::Column::TrackNumber)
        .all(db)
        .await?;

    for target_track in target_tracks {
        let target_links = load_track_provider_links(db, target_track.id).await?;

        let shares_provider_identity = source_links.iter().any(|source_link| {
            target_links.iter().any(|target_link| {
                target_link.provider == source_link.provider
                    && target_link.provider_track_id == source_link.provider_track_id
            })
        });
        if shares_provider_identity {
            return Ok(Some(target_track));
        }

        if source_track.isrc.is_some() && source_track.isrc == target_track.isrc {
            return Ok(Some(target_track));
        }

        if tracks_share_identity(source_track, &target_track) {
            return Ok(Some(target_track));
        }
    }

    Ok(None)
}

async fn load_track_provider_links<C>(
    db: &C,
    track_id: Uuid,
) -> AppResult<Vec<db::track_provider_link::Model>>
where
    C: sea_orm::ConnectionTrait,
{
    Ok(db::track_provider_link::Entity::find()
        .filter(db::track_provider_link::Column::TrackId.eq(track_id))
        .all(db)
        .await?)
}

fn tracks_share_identity(left: &db::track::Model, right: &db::track::Model) -> bool {
    left.disc_number == right.disc_number
        && left.track_number == right.track_number
        && normalize(&left.title) == normalize(&right.title)
        && normalize_optional(&left.version) == normalize_optional(&right.version)
}

fn normalize_optional(value: &Option<String>) -> String {
    value.as_deref().map(normalize).unwrap_or_default()
}

fn combined_album_status(statuses: impl Iterator<Item = WantedStatus>) -> WantedStatus {
    statuses
        .max_by_key(wanted_status_rank)
        .unwrap_or(WantedStatus::Unmonitored)
}

fn prefer_track_status(left: WantedStatus, right: WantedStatus) -> WantedStatus {
    if wanted_status_rank(&right) > wanted_status_rank(&left) {
        right
    } else {
        left
    }
}

fn wanted_status_rank(status: &WantedStatus) -> u8 {
    match status {
        WantedStatus::Unmonitored => 0,
        WantedStatus::Wanted => 1,
        WantedStatus::InProgress => 2,
        WantedStatus::Acquired => 3,
    }
}

fn prefer_album_quality(left: Option<Quality>, right: Option<Quality>) -> Option<Quality> {
    match (left, right) {
        (Some(left), Some(right)) => {
            if quality_rank(right) > quality_rank(left) {
                Some(right)
            } else {
                Some(left)
            }
        }
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn quality_rank(value: Quality) -> u8 {
    match value {
        Quality::Low => 0,
        Quality::High => 1,
        Quality::Lossless => 2,
        Quality::HiRes => 3,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::NaiveDate;
    use sea_orm::{
        ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait,
        QueryFilter, QueryOrder,
    };

    use super::merge_albums;
    use crate::{
        app_config::AuthConfig,
        db::{
            self, album, album_artist, album_provider_link, album_type::AlbumType, download_job,
            download_status::DownloadStatus, provider::Provider, quality::Quality, track,
            track_provider_link, wanted_status::WantedStatus,
        },
        providers::registry::ProviderRegistry,
        state::AppState,
    };

    async fn test_state() -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-merge-test-{}.db?mode=rwc",
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

    #[tokio::test]
    async fn merge_albums_moves_links_tracks_and_jobs() {
        let state = test_state().await;

        let artist = db::artist::ActiveModel {
            name: Set("Artist".to_string()),
            monitored: Set(true),
            image_url: Set(None),
            ..db::artist::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert artist");

        let guest = db::artist::ActiveModel {
            name: Set("Guest".to_string()),
            monitored: Set(false),
            image_url: Set(None),
            ..db::artist::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert guest");

        let target_album = album::ActiveModel {
            title: Set("Target Album".to_string()),
            album_type: Set(AlbumType::Unknown),
            release_date: Set(None),
            cover_url: Set(None),
            explicit: Set(false),
            wanted_status: Set(WantedStatus::Unmonitored),
            requested_quality: Set(None),
            ..album::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert target album");

        let source_album = album::ActiveModel {
            title: Set("Source Album".to_string()),
            album_type: Set(AlbumType::Album),
            release_date: Set(Some(NaiveDate::from_ymd_opt(2024, 3, 12).expect("date"))),
            cover_url: Set(Some("https://example.com/source.jpg".to_string())),
            explicit: Set(true),
            wanted_status: Set(WantedStatus::Acquired),
            requested_quality: Set(Some(Quality::HiRes)),
            ..album::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert source album");

        album_artist::ActiveModel {
            album_id: Set(target_album.id),
            artist_id: Set(artist.id),
            priority: Set(0),
        }
        .insert(&state.db)
        .await
        .expect("link target artist");
        album_artist::ActiveModel {
            album_id: Set(source_album.id),
            artist_id: Set(artist.id),
            priority: Set(0),
        }
        .insert(&state.db)
        .await
        .expect("link source artist");
        album_artist::ActiveModel {
            album_id: Set(source_album.id),
            artist_id: Set(guest.id),
            priority: Set(1),
        }
        .insert(&state.db)
        .await
        .expect("link source guest");

        album_provider_link::ActiveModel {
            album_id: Set(target_album.id),
            provider: Set(Provider::Tidal),
            provider_album_id: Set("shared-album".to_string()),
            external_url: Set(None),
            external_name: Set(Some("Target Album".to_string())),
            ..album_provider_link::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert target provider link");
        album_provider_link::ActiveModel {
            album_id: Set(source_album.id),
            provider: Set(Provider::Tidal),
            provider_album_id: Set("shared-album".to_string()),
            external_url: Set(None),
            external_name: Set(Some("Source Album".to_string())),
            ..album_provider_link::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert duplicate source provider link");
        album_provider_link::ActiveModel {
            album_id: Set(source_album.id),
            provider: Set(Provider::Deezer),
            provider_album_id: Set("source-deezer".to_string()),
            external_url: Set(None),
            external_name: Set(Some("Source Album".to_string())),
            ..album_provider_link::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert unique source provider link");

        let target_track = track::ActiveModel {
            title: Set("Track One".to_string()),
            version: Set(None),
            disc_number: Set(Some(1)),
            track_number: Set(Some(1)),
            duration: Set(Some(180)),
            album_id: Set(target_album.id),
            explicit: Set(false),
            isrc: Set(Some("same-isrc".to_string())),
            root_folder_id: Set(None),
            status: Set(WantedStatus::Wanted),
            file_path: Set(None),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert target track");

        track_provider_link::ActiveModel {
            track_id: Set(target_track.id),
            provider: Set(Provider::Tidal),
            provider_track_id: Set("target-tidal-track".to_string()),
            ..track_provider_link::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert target track link");

        let source_duplicate_track = track::ActiveModel {
            title: Set("Track One".to_string()),
            version: Set(None),
            disc_number: Set(Some(1)),
            track_number: Set(Some(1)),
            duration: Set(Some(181)),
            album_id: Set(source_album.id),
            explicit: Set(true),
            isrc: Set(Some("same-isrc".to_string())),
            root_folder_id: Set(None),
            status: Set(WantedStatus::Acquired),
            file_path: Set(Some("/music/source-track.flac".to_string())),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert duplicate source track");

        track_provider_link::ActiveModel {
            track_id: Set(source_duplicate_track.id),
            provider: Set(Provider::Deezer),
            provider_track_id: Set("source-deezer-track".to_string()),
            ..track_provider_link::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert source duplicate track link");

        let source_unique_track = track::ActiveModel {
            title: Set("Track Two".to_string()),
            version: Set(Some("Live".to_string())),
            disc_number: Set(Some(1)),
            track_number: Set(Some(2)),
            duration: Set(Some(240)),
            album_id: Set(source_album.id),
            explicit: Set(false),
            isrc: Set(Some("unique-isrc".to_string())),
            root_folder_id: Set(None),
            status: Set(WantedStatus::Wanted),
            file_path: Set(None),
            ..track::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert unique source track");

        download_job::ActiveModel {
            album_id: Set(source_album.id),
            source: Set(Provider::Tidal),
            quality: Set(Quality::HiRes),
            status: Set(DownloadStatus::Queued),
            total_tracks: Set(2),
            completed_tasks: Set(0),
            ..download_job::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert source job");

        merge_albums(
            &state,
            target_album.id,
            source_album.id,
            Some("Merged Album"),
            None,
        )
        .await
        .expect("merge albums");

        assert!(
            db::album::Entity::find_by_id(source_album.id)
                .one(&state.db)
                .await
                .expect("reload source album")
                .is_none()
        );

        let merged_album = db::album::Entity::find_by_id(target_album.id)
            .one(&state.db)
            .await
            .expect("reload target album")
            .expect("target album exists");
        assert_eq!(merged_album.title, "Merged Album");
        assert_eq!(merged_album.album_type, AlbumType::Album);
        assert_eq!(
            merged_album.release_date,
            Some(NaiveDate::from_ymd_opt(2024, 3, 12).expect("date"))
        );
        assert_eq!(
            merged_album.cover_url.as_deref(),
            Some("https://example.com/source.jpg")
        );
        assert!(merged_album.explicit);
        assert_eq!(merged_album.wanted_status, WantedStatus::Acquired);
        assert_eq!(merged_album.requested_quality, Some(Quality::HiRes));

        let provider_links = db::album_provider_link::Entity::find()
            .filter(db::album_provider_link::Column::AlbumId.eq(target_album.id))
            .all(&state.db)
            .await
            .expect("reload provider links");
        assert_eq!(provider_links.len(), 2);
        assert!(provider_links.iter().any(|link| {
            link.provider == Provider::Tidal && link.provider_album_id == "shared-album"
        }));
        assert!(provider_links.iter().any(|link| {
            link.provider == Provider::Deezer && link.provider_album_id == "source-deezer"
        }));

        let album_artists = db::album_artist::Entity::find_by_album_ordered(target_album.id)
            .all(&state.db)
            .await
            .expect("reload album artists");
        assert_eq!(album_artists.len(), 2);
        assert!(album_artists.iter().any(|link| link.artist_id == artist.id));
        assert!(album_artists.iter().any(|link| link.artist_id == guest.id));

        let tracks = db::track::Entity::find()
            .filter(db::track::Column::AlbumId.eq(target_album.id))
            .order_by_asc(db::track::Column::TrackNumber)
            .all(&state.db)
            .await
            .expect("reload tracks");
        assert_eq!(tracks.len(), 2);

        let merged_duplicate_track = tracks
            .iter()
            .find(|track| track.track_number == Some(1))
            .expect("merged duplicate track");
        assert_eq!(merged_duplicate_track.status, WantedStatus::Acquired);
        assert_eq!(
            merged_duplicate_track.file_path.as_deref(),
            Some("/music/source-track.flac")
        );
        assert!(merged_duplicate_track.explicit);

        let merged_duplicate_links = db::track_provider_link::Entity::find()
            .filter(db::track_provider_link::Column::TrackId.eq(merged_duplicate_track.id))
            .all(&state.db)
            .await
            .expect("reload merged duplicate links");
        assert_eq!(merged_duplicate_links.len(), 2);
        assert!(merged_duplicate_links.iter().any(|link| {
            link.provider == Provider::Tidal && link.provider_track_id == "target-tidal-track"
        }));
        assert!(merged_duplicate_links.iter().any(|link| {
            link.provider == Provider::Deezer && link.provider_track_id == "source-deezer-track"
        }));

        let moved_track = tracks
            .iter()
            .find(|track| track.track_number == Some(2))
            .expect("moved unique track");
        assert_eq!(moved_track.id, source_unique_track.id);
        assert_eq!(moved_track.album_id, target_album.id);

        let jobs = db::download_job::Entity::find()
            .filter(db::download_job::Column::AlbumId.eq(target_album.id))
            .all(&state.db)
            .await
            .expect("reload jobs");
        assert_eq!(jobs.len(), 1);
    }
}
