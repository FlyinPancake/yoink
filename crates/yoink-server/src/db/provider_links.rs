use sqlx::SqlitePool;
use uuid::Uuid;

// ── Artist provider links ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct ArtistProviderLink {
    pub(crate) id: Uuid,
    pub(crate) artist_id: Uuid,
    pub(crate) provider: String,
    pub(crate) external_id: String,
    pub(crate) external_url: Option<String>,
    pub(crate) external_name: Option<String>,
    pub(crate) image_ref: Option<String>,
}

pub(crate) async fn upsert_artist_provider_link(
    pool: &SqlitePool,
    link: &ArtistProviderLink,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO artist_provider_links (id, artist_id, provider, external_id, external_url, external_name, image_ref)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT(provider, external_id) DO UPDATE SET
           external_url = excluded.external_url,
           external_name = excluded.external_name,
           image_ref = excluded.image_ref",
        link.id, link.artist_id, link.provider, link.external_id, link.external_url, link.external_name, link.image_ref,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn delete_artist_provider_link(
    pool: &SqlitePool,
    artist_id: Uuid,
    provider: &str,
    external_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM artist_provider_links WHERE artist_id = $1 AND provider = $2 AND external_id = $3",
        artist_id, provider, external_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn load_artist_provider_links(
    pool: &SqlitePool,
    artist_id: Uuid,
) -> Result<Vec<ArtistProviderLink>, sqlx::Error> {
    sqlx::query_as!(
        ArtistProviderLink,
        r#"SELECT
            id as "id!: Uuid",
            artist_id as "artist_id!: Uuid",
            provider, external_id, external_url, external_name, image_ref
         FROM artist_provider_links WHERE artist_id = $1"#,
        artist_id,
    )
    .fetch_all(pool)
    .await
}

/// Find a local artist by a provider link's external_id.
pub(crate) async fn find_artist_by_provider_link(
    pool: &SqlitePool,
    provider: &str,
    external_id: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT artist_id as "artist_id!: Uuid"
           FROM artist_provider_links WHERE provider = $1 AND external_id = $2"#,
        provider,
        external_id,
    )
    .fetch_optional(pool)
    .await
}

// ── Album provider links ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct AlbumProviderLink {
    pub(crate) id: Uuid,
    pub(crate) album_id: Uuid,
    pub(crate) provider: String,
    pub(crate) external_id: String,
    pub(crate) external_url: Option<String>,
    pub(crate) external_title: Option<String>,
    pub(crate) cover_ref: Option<String>,
}

pub(crate) async fn upsert_album_provider_link(
    pool: &SqlitePool,
    link: &AlbumProviderLink,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO album_provider_links (id, album_id, provider, external_id, external_url, external_title, cover_ref)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT(provider, external_id) DO UPDATE SET
           album_id = excluded.album_id,
           external_url = excluded.external_url,
           external_title = excluded.external_title,
           cover_ref = excluded.cover_ref",
        link.id, link.album_id, link.provider, link.external_id, link.external_url, link.external_title, link.cover_ref,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn load_album_provider_links(
    pool: &SqlitePool,
    album_id: Uuid,
) -> Result<Vec<AlbumProviderLink>, sqlx::Error> {
    sqlx::query_as!(
        AlbumProviderLink,
        r#"SELECT
            id as "id!: Uuid",
            album_id as "album_id!: Uuid",
            provider, external_id, external_url, external_title, cover_ref
         FROM album_provider_links WHERE album_id = $1"#,
        album_id,
    )
    .fetch_all(pool)
    .await
}

/// Find a local album by a provider link's external_id.
pub(crate) async fn find_album_by_provider_link(
    pool: &SqlitePool,
    provider: &str,
    external_id: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT album_id as "album_id!: Uuid"
           FROM album_provider_links WHERE provider = $1 AND external_id = $2"#,
        provider,
        external_id,
    )
    .fetch_optional(pool)
    .await
}

// ── Track provider links ────────────────────────────────────────────

#[allow(dead_code)]
pub(crate) async fn upsert_track_provider_link(
    pool: &SqlitePool,
    track_id: Uuid,
    provider: &str,
    external_id: &str,
) -> Result<(), sqlx::Error> {
    let link_id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO track_provider_links (id, track_id, provider, external_id)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT(provider, external_id) DO UPDATE SET
           track_id = excluded.track_id",
        link_id,
        track_id,
        provider,
        external_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Find a local track by a provider link's external_id.
pub(crate) async fn find_track_by_provider_link(
    pool: &SqlitePool,
    provider: &str,
    external_id: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT track_id as "track_id!: Uuid"
           FROM track_provider_links WHERE provider = $1 AND external_id = $2"#,
        provider,
        external_id,
    )
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::*;

    #[tokio::test]
    async fn artist_provider_link_crud() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;

        let link_id = seed_artist_provider_link(&pool, artist.id, "tidal", "EXT123").await;

        let links = super::load_artist_provider_links(&pool, artist.id)
            .await
            .unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].id, link_id);
        assert_eq!(links[0].provider, "tidal");
        assert_eq!(links[0].external_id, "EXT123");
    }

    #[tokio::test]
    async fn artist_provider_link_upsert_updates_on_conflict() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;

        let link = super::ArtistProviderLink {
            id: uuid::Uuid::now_v7(),
            artist_id: artist.id,
            provider: "tidal".to_string(),
            external_id: "EXT1".to_string(),
            external_url: None,
            external_name: Some("Old Name".to_string()),
            image_ref: None,
        };
        super::upsert_artist_provider_link(&pool, &link)
            .await
            .unwrap();

        // Upsert again with same (provider, external_id) but updated name
        let updated = super::ArtistProviderLink {
            id: uuid::Uuid::now_v7(), // different id
            external_name: Some("New Name".to_string()),
            ..link.clone()
        };
        super::upsert_artist_provider_link(&pool, &updated)
            .await
            .unwrap();

        let links = super::load_artist_provider_links(&pool, artist.id)
            .await
            .unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].external_name.as_deref(), Some("New Name"));
    }

    #[tokio::test]
    async fn find_artist_by_provider_link() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        seed_artist_provider_link(&pool, artist.id, "deezer", "D456").await;

        let found = super::find_artist_by_provider_link(&pool, "deezer", "D456")
            .await
            .unwrap();
        assert_eq!(found, Some(artist.id));

        let not_found = super::find_artist_by_provider_link(&pool, "deezer", "NOPE")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn delete_artist_provider_link() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        seed_artist_provider_link(&pool, artist.id, "tidal", "T1").await;
        seed_artist_provider_link(&pool, artist.id, "deezer", "D1").await;

        super::delete_artist_provider_link(&pool, artist.id, "tidal", "T1")
            .await
            .unwrap();

        let links = super::load_artist_provider_links(&pool, artist.id)
            .await
            .unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].provider, "deezer");
    }

    #[tokio::test]
    async fn album_provider_link_crud() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;

        let link_id = seed_album_provider_link(&pool, album.id, "tidal", "ALB123").await;

        let links = super::load_album_provider_links(&pool, album.id)
            .await
            .unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].id, link_id);
        assert_eq!(links[0].provider, "tidal");
        assert_eq!(links[0].external_id, "ALB123");
    }

    #[tokio::test]
    async fn find_album_by_provider_link() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        seed_album_provider_link(&pool, album.id, "deezer", "DALB1").await;

        let found = super::find_album_by_provider_link(&pool, "deezer", "DALB1")
            .await
            .unwrap();
        assert_eq!(found, Some(album.id));

        let not_found = super::find_album_by_provider_link(&pool, "deezer", "NOPE")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn track_provider_link_upsert_and_find() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album = seed_album(&pool, artist.id, "Album").await;
        let tracks = seed_tracks(&pool, album.id, 1).await;

        super::upsert_track_provider_link(&pool, tracks[0].id, "tidal", "TRK1")
            .await
            .unwrap();

        let found = crate::db::find_track_by_provider_link(&pool, "tidal", "TRK1")
            .await
            .unwrap();
        assert_eq!(found, Some(tracks[0].id));
    }

    #[tokio::test]
    async fn album_provider_link_unique_constraint() {
        let pool = test_db().await;
        let artist = seed_artist(&pool, "Artist").await;
        let album1 = seed_album(&pool, artist.id, "Album 1").await;
        let album2 = seed_album(&pool, artist.id, "Album 2").await;

        seed_album_provider_link(&pool, album1.id, "tidal", "SAME_EXT").await;

        // Upserting with same (provider, external_id) should update album_id
        let link = super::AlbumProviderLink {
            id: uuid::Uuid::now_v7(),
            album_id: album2.id,
            provider: "tidal".to_string(),
            external_id: "SAME_EXT".to_string(),
            external_url: None,
            external_title: None,
            cover_ref: None,
        };
        super::upsert_album_provider_link(&pool, &link)
            .await
            .unwrap();

        // Should now point to album2
        let found = super::find_album_by_provider_link(&pool, "tidal", "SAME_EXT")
            .await
            .unwrap();
        assert_eq!(found, Some(album2.id));
    }
}
