use sqlx::SqlitePool;
use uuid::Uuid;
use yoink_shared::Quality;

use crate::models::TrackInfo;

/// Row struct for tracks — `duration_display` is computed after load.
struct TrackRow {
    id: Uuid,
    title: String,
    version: Option<String>,
    disc_number: i64,
    track_number: i64,
    duration_secs: Option<i64>,
    explicit: bool,
    isrc: Option<String>,
    quality_override: Option<String>,
    track_artist: Option<String>,
    file_path: Option<String>,
    monitored: bool,
    acquired: bool,
}

impl From<TrackRow> for TrackInfo {
    fn from(r: TrackRow) -> Self {
        let secs = r.duration_secs.unwrap_or(0) as u32;
        Self {
            id: r.id,
            title: r.title,
            version: r.version,
            disc_number: r.disc_number as u32,
            track_number: r.track_number as u32,
            duration_secs: secs,
            duration_display: yoink_shared::format_duration(secs),
            isrc: r.isrc,
            explicit: r.explicit,
            quality_override: r
                .quality_override
                .as_deref()
                .map(str::parse)
                .transpose()
                .expect("expected valid track quality override"),
            track_artist: r.track_artist,
            file_path: r.file_path,
            monitored: r.monitored,
            acquired: r.acquired,
        }
    }
}

pub(crate) async fn load_tracks_for_album(
    pool: &SqlitePool,
    album_id: Uuid,
) -> Result<Vec<TrackInfo>, sqlx::Error> {
    let rows = sqlx::query_as!(
        TrackRow,
        r#"SELECT
            id as "id!: Uuid",
            title, version, disc_number, track_number, duration_secs,
            explicit as "explicit!: bool",
            isrc, quality_override, track_artist, file_path,
            monitored as "monitored!: bool",
            acquired as "acquired!: bool"
         FROM tracks WHERE album_id = $1
         ORDER BY disc_number, track_number"#,
        album_id,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(TrackInfo::from).collect())
}

pub(crate) async fn upsert_track(
    pool: &SqlitePool,
    track: &TrackInfo,
    album_id: Uuid,
) -> Result<(), sqlx::Error> {
    let disc = track.disc_number as i32;
    let tnum = track.track_number as i32;
    let dur = track.duration_secs as i32;
    let quality_override = track.quality_override.map(|q| q.as_str().to_string());
    sqlx::query!(
        "INSERT INTO tracks (id, album_id, title, version, disc_number, track_number, duration_secs, explicit, isrc, quality_override, track_artist, file_path, monitored, acquired)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         ON CONFLICT(id) DO UPDATE SET
           title = excluded.title,
           version = excluded.version,
           disc_number = excluded.disc_number,
           track_number = excluded.track_number,
           duration_secs = excluded.duration_secs,
           explicit = excluded.explicit,
           isrc = excluded.isrc,
           quality_override = excluded.quality_override,
           track_artist = COALESCE(excluded.track_artist, tracks.track_artist),
           file_path = COALESCE(excluded.file_path, tracks.file_path),
           monitored = excluded.monitored,
           acquired = excluded.acquired",
        track.id, album_id, track.title, track.version, disc, tnum, dur, track.explicit, track.isrc,
        quality_override, track.track_artist, track.file_path, track.monitored, track.acquired,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Update only the monitored and acquired flags for a track.
pub(crate) async fn update_track_flags(
    pool: &SqlitePool,
    track_id: Uuid,
    monitored: bool,
    acquired: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE tracks SET monitored = $1, acquired = $2 WHERE id = $3",
        monitored,
        acquired,
        track_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Load all monitored tracks for an album (tracks individually selected for download).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn load_monitored_tracks_for_album(
    pool: &SqlitePool,
    album_id: Uuid,
) -> Result<Vec<TrackInfo>, sqlx::Error> {
    let rows = sqlx::query_as!(
        TrackRow,
        r#"SELECT
            id as "id!: Uuid",
            title, version, disc_number, track_number, duration_secs,
            explicit as "explicit!: bool",
            isrc, quality_override, track_artist, file_path,
            monitored as "monitored!: bool",
            acquired as "acquired!: bool"
         FROM tracks WHERE album_id = $1 AND monitored = 1
         ORDER BY disc_number, track_number"#,
        album_id,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(TrackInfo::from).collect())
}

pub(crate) async fn update_track_quality_override(
    pool: &SqlitePool,
    track_id: Uuid,
    quality: Option<Quality>,
) -> Result<(), sqlx::Error> {
    let quality_override = quality.map(|q| q.as_str().to_string());
    sqlx::query!(
        "UPDATE tracks SET quality_override = $1 WHERE id = $2",
        quality_override,
        track_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Check if an album has any individually monitored tracks that are not yet acquired.
pub(crate) async fn has_wanted_tracks(
    pool: &SqlitePool,
    album_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "count!: i64"
           FROM tracks WHERE album_id = $1 AND monitored = 1 AND acquired = 0"#,
        album_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

pub(crate) async fn find_track_by_album_isrc(
    pool: &SqlitePool,
    album_id: Uuid,
    isrc: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT id as "id!: Uuid"
           FROM tracks WHERE album_id = $1 AND UPPER(isrc) = UPPER($2) LIMIT 1"#,
        album_id,
        isrc,
    )
    .fetch_optional(pool)
    .await
}

pub(crate) async fn find_track_by_album_position(
    pool: &SqlitePool,
    album_id: Uuid,
    disc_number: u32,
    track_number: u32,
) -> Result<Option<Uuid>, sqlx::Error> {
    let disc = disc_number as i32;
    let tnum = track_number as i32;
    sqlx::query_scalar!(
        r#"SELECT id as "id!: Uuid"
           FROM tracks
           WHERE album_id = $1 AND disc_number = $2 AND track_number = $3
           LIMIT 1"#,
        album_id,
        disc,
        tnum,
    )
    .fetch_optional(pool)
    .await
}

/// Check whether ALL monitored tracks for an album have been acquired.
/// Returns `true` when there are no monitored-but-unacquired tracks,
/// including the degenerate case where no tracks are monitored at all.
pub(crate) async fn all_monitored_tracks_acquired(
    pool: &SqlitePool,
    album_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "count!: i64"
           FROM tracks WHERE album_id = $1 AND monitored = 1 AND acquired = 0"#,
        album_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(count == 0)
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::*;
    use yoink_shared::Quality;

    #[tokio::test]
    async fn upsert_and_load_tracks() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 3).await;

        let loaded = super::load_tracks_for_album(&pool, album.id).await.unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].id, tracks[0].id);
        assert_eq!(loaded[0].title, "Track 1");
        assert_eq!(loaded[0].track_number, 1);
        assert_eq!(loaded[2].track_number, 3);
    }

    #[tokio::test]
    async fn tracks_ordered_by_disc_and_number() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;

        // Insert tracks out of order, across 2 discs
        for (disc, num) in [(2, 1), (1, 3), (1, 1), (1, 2), (2, 2)] {
            let track = crate::models::TrackInfo {
                id: uuid::Uuid::now_v7(),
                title: format!("D{disc}T{num}"),
                version: None,
                disc_number: disc,
                track_number: num,
                duration_secs: 200,
                duration_display: "3:20".to_string(),
                isrc: None,
                explicit: false,
                quality_override: None,
                track_artist: None,
                file_path: None,
                monitored: false,
                acquired: false,
            };
            crate::db::upsert_track(&pool, &track, album.id)
                .await
                .unwrap();
        }

        let loaded = super::load_tracks_for_album(&pool, album.id).await.unwrap();
        let order: Vec<String> = loaded.iter().map(|t| t.title.clone()).collect();
        assert_eq!(order, vec!["D1T1", "D1T2", "D1T3", "D2T1", "D2T2"]);
    }

    #[tokio::test]
    async fn update_track_flags() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 1).await;

        super::update_track_flags(&pool, tracks[0].id, true, true)
            .await
            .unwrap();

        let loaded = super::load_tracks_for_album(&pool, album.id).await.unwrap();
        assert!(loaded[0].monitored);
        assert!(loaded[0].acquired);
    }

    #[tokio::test]
    async fn has_wanted_tracks_true_when_monitored_unacquired() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 2).await;

        // Initially no tracks are monitored
        assert!(!super::has_wanted_tracks(&pool, album.id).await.unwrap());

        // Monitor one track (but don't acquire it)
        super::update_track_flags(&pool, tracks[0].id, true, false)
            .await
            .unwrap();
        assert!(super::has_wanted_tracks(&pool, album.id).await.unwrap());
    }

    #[tokio::test]
    async fn has_wanted_tracks_false_when_all_acquired() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 1).await;

        super::update_track_flags(&pool, tracks[0].id, true, true)
            .await
            .unwrap();
        assert!(!super::has_wanted_tracks(&pool, album.id).await.unwrap());
    }

    #[tokio::test]
    async fn all_monitored_tracks_acquired_empty() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        seed_tracks(&pool, album.id, 2).await;

        // No tracks monitored at all → degenerate true
        assert!(
            super::all_monitored_tracks_acquired(&pool, album.id)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn all_monitored_tracks_acquired_partial() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 2).await;

        // Monitor both, acquire only one
        super::update_track_flags(&pool, tracks[0].id, true, true)
            .await
            .unwrap();
        super::update_track_flags(&pool, tracks[1].id, true, false)
            .await
            .unwrap();
        assert!(
            !super::all_monitored_tracks_acquired(&pool, album.id)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn all_monitored_tracks_acquired_all() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 2).await;

        super::update_track_flags(&pool, tracks[0].id, true, true)
            .await
            .unwrap();
        super::update_track_flags(&pool, tracks[1].id, true, true)
            .await
            .unwrap();
        assert!(
            super::all_monitored_tracks_acquired(&pool, album.id)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn load_monitored_tracks_for_album() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 3).await;

        super::update_track_flags(&pool, tracks[1].id, true, false)
            .await
            .unwrap();

        let monitored = super::load_monitored_tracks_for_album(&pool, album.id)
            .await
            .unwrap();
        assert_eq!(monitored.len(), 1);
        assert_eq!(monitored[0].id, tracks[1].id);
    }

    #[tokio::test]
    async fn upsert_track_coalesce_preserves_existing_track_artist() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;

        let mut track = crate::models::TrackInfo {
            id: uuid::Uuid::now_v7(),
            title: "Song".to_string(),
            version: None,
            disc_number: 1,
            track_number: 1,
            duration_secs: 200,
            duration_display: "3:20".to_string(),
            isrc: None,
            explicit: false,
            quality_override: None,
            track_artist: Some("Featured Artist".to_string()),
            file_path: Some("/music/song.flac".to_string()),
            monitored: false,
            acquired: false,
        };
        crate::db::upsert_track(&pool, &track, album.id)
            .await
            .unwrap();

        // Upsert again with None for track_artist and file_path
        // The COALESCE should preserve the existing values
        track.track_artist = None;
        track.file_path = None;
        track.title = "Song (Remastered)".to_string();
        crate::db::upsert_track(&pool, &track, album.id)
            .await
            .unwrap();

        let loaded = super::load_tracks_for_album(&pool, album.id).await.unwrap();
        assert_eq!(loaded[0].title, "Song (Remastered)");
        assert_eq!(loaded[0].track_artist.as_deref(), Some("Featured Artist"));
        assert_eq!(loaded[0].file_path.as_deref(), Some("/music/song.flac"));
    }

    #[tokio::test]
    async fn track_quality_override_roundtrips_and_updates() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 1).await;

        super::update_track_quality_override(&pool, tracks[0].id, Some(Quality::Low))
            .await
            .unwrap();
        let loaded = super::load_tracks_for_album(&pool, album.id).await.unwrap();
        assert_eq!(loaded[0].quality_override, Some(Quality::Low));

        super::update_track_quality_override(&pool, tracks[0].id, None)
            .await
            .unwrap();
        let loaded = super::load_tracks_for_album(&pool, album.id).await.unwrap();
        assert_eq!(loaded[0].quality_override, None);
    }

    #[tokio::test]
    async fn find_track_by_album_isrc() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 2).await;

        // seed_tracks generates ISRCs like "USRC12340001"
        let found = super::find_track_by_album_isrc(&pool, album.id, "USRC12340001")
            .await
            .unwrap();
        assert_eq!(found, Some(tracks[0].id));

        // Case insensitive
        let found = super::find_track_by_album_isrc(&pool, album.id, "usrc12340001")
            .await
            .unwrap();
        assert_eq!(found, Some(tracks[0].id));

        // Non-existent
        let found = super::find_track_by_album_isrc(&pool, album.id, "NOTEXIST")
            .await
            .unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn find_track_by_album_position() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 3).await;

        let found = super::find_track_by_album_position(&pool, album.id, 1, 2)
            .await
            .unwrap();
        assert_eq!(found, Some(tracks[1].id));

        // Non-existent position
        let found = super::find_track_by_album_position(&pool, album.id, 1, 99)
            .await
            .unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn find_track_by_provider_link() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 1).await;

        crate::db::upsert_track_provider_link(&pool, tracks[0].id, "tidal", "T123")
            .await
            .unwrap();

        let found = crate::db::find_track_by_provider_link(&pool, "tidal", "T123")
            .await
            .unwrap();
        assert_eq!(found, Some(tracks[0].id));

        let not_found = crate::db::find_track_by_provider_link(&pool, "tidal", "NOPE")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }
}
