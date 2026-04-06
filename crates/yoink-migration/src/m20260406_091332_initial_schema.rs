use sea_orm_migration::{prelude::*, schema::*};

const QUALITY_VALUES: &[&str] = &["low", "high", "lossless", "hi_res"];
const WANTED_STATUS_VALUES: &[&str] = &["unmonitored", "wanted", "in_progress", "acquired"];
const PROVIDER_VALUES: &[&str] = &["tidal", "deezer", "music_brainz", "soulseek"];
const PROVIDER_VALUES_WITH_NONE: &[&str] = &["tidal", "deezer", "music_brainz", "soulseek", "none"];
const MATCH_KIND_VALUES: &[&str] = &["fuzzy", "isrc_exact"];
const MATCH_STATUS_VALUES: &[&str] = &["pending", "accepted", "dismissed"];
const DOWNLOAD_STATUS_VALUES: &[&str] =
    &["queued", "resolving", "downloading", "completed", "failed"];
const ALBUM_TYPE_VALUES: &[&str] = &["album", "ep", "single", "unknown"];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Root Folder

        manager
            .create_table(
                Table::create()
                    .table("root_folders")
                    .col(pk_uuid("id"))
                    .col(string("path"))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .to_owned(),
            )
            .await?;

        // Artist

        manager
            .create_table(
                Table::create()
                    .table("artists")
                    .col(pk_uuid("id"))
                    .col(string("name"))
                    .col(string_null("image_url"))
                    .col(string_null("bio"))
                    .col(boolean("monitored"))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-artists-name")
                    .table("artists")
                    .col("name")
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("artist_match_candidates")
                    .col(pk_uuid("id"))
                    .col(uuid("artist_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-artist-match-candidates-artists")
                            .from("artist_match_candidates", "artist_id")
                            .to("artists", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(string("left_provider"))
                    .col(string("left_external_id"))
                    .col(string("right_provider"))
                    .col(string("right_external_id"))
                    .col(string("match_kind"))
                    .col(integer("confidence"))
                    .col(string_null("explanation"))
                    .col(string_null("external_name"))
                    .col(string_null("external_url"))
                    .col(string_null("image_url"))
                    .col(string_null("disambiguation"))
                    .col(string_null("artist_type"))
                    .col(string_null("country"))
                    .col(string_null("tags_json"))
                    .col(integer_null("popularity"))
                    .col(string("status"))
                    .check(provider_check(
                        "ck-artist-match-candidates-left-provider",
                        "left_provider",
                    ))
                    .check(provider_check(
                        "ck-artist-match-candidates-right-provider",
                        "right_provider",
                    ))
                    .check(match_kind_check(
                        "ck-artist-match-candidates-match-kind",
                        "match_kind",
                    ))
                    .check(match_status_check(
                        "ck-artist-match-candidates-status",
                        "status",
                    ))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-artist-match-candidates-artist-id")
                    .table("artist_match_candidates")
                    .col("artist_id")
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("artist_provider_links")
                    .col(pk_uuid("id"))
                    .col(uuid("artist_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-artist-provider-links-artists")
                            .from("artist_provider_links", "artist_id")
                            .to("artists", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(string("provider"))
                    .col(string("external_id"))
                    .col(string_null("external_url"))
                    .col(string_null("external_name"))
                    .check(provider_check(
                        "ck-artist-provider-links-provider",
                        "provider",
                    ))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-artist-provider-links-artist-id")
                    .table("artist_provider_links")
                    .col("artist_id")
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-artist-provider-links-external-id")
                    .table("artist_provider_links")
                    .col("external_id")
                    .to_owned(),
            )
            .await?;

        // Albums

        manager
            .create_table(
                Table::create()
                    .table("albums")
                    .col(pk_uuid("id"))
                    .col(string("title"))
                    .col(string("album_type"))
                    .col(date_null("release_date"))
                    .col(string_null("cover_url"))
                    .col(boolean("explicit"))
                    .col(string("wanted_status"))
                    .col(string_null("requested_quality"))
                    .check(album_type_check("ck-albums-album-type", "album_type"))
                    .check(wanted_status_check(
                        "ck-albums-wanted-status",
                        "wanted_status",
                    ))
                    .check(quality_check(
                        "ck-albums-requested-quality",
                        "requested_quality",
                    ))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-albums-title")
                    .table("albums")
                    .col("title")
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("album_provider_links")
                    .col(pk_uuid("id"))
                    .col(uuid("album_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-album-provider-links-album")
                            .from("album_provider_links", "album_id")
                            .to("albums", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(string("provider"))
                    .col(string("provider_album_id"))
                    .col(string_null("external_url"))
                    .col(string_null("external_name"))
                    .check(provider_check(
                        "ck-album-provider-links-provider",
                        "provider",
                    ))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-album-provider-links-album-id")
                    .table("album_provider_links")
                    .col("album_id")
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("album_match_candidates")
                    .col(pk_uuid("id"))
                    .col(uuid("album_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-album-match-candidates-album")
                            .from("album_match_candidates", "album_id")
                            .to("albums", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(string("left_provider"))
                    .col(string("left_external_id"))
                    .col(string("right_provider"))
                    .col(string("right_external_id"))
                    .col(string("match_kind"))
                    .col(integer("confidence"))
                    .col(string_null("explanation"))
                    .col(string_null("external_name"))
                    .col(string_null("external_url"))
                    .col(string_null("image_url"))
                    .col(string_null("tags_json"))
                    .col(integer_null("popularity"))
                    .col(string("status"))
                    .check(provider_check(
                        "ck-album-match-candidates-left-provider",
                        "left_provider",
                    ))
                    .check(provider_check(
                        "ck-album-match-candidates-right-provider",
                        "right_provider",
                    ))
                    .check(match_kind_check(
                        "ck-album-match-candidates-match-kind",
                        "match_kind",
                    ))
                    .check(match_status_check(
                        "ck-album-match-candidates-status",
                        "status",
                    ))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-album-match-candidates-album-id")
                    .table("album_match_candidates")
                    .col("album_id")
                    .to_owned(),
            )
            .await?;

        // Tracks

        manager
            .create_table(
                Table::create()
                    .table("tracks")
                    .col(pk_uuid("id"))
                    .col(string("title"))
                    .col(string_null("version"))
                    .col(integer_null("disc_number"))
                    .col(integer_null("track_number"))
                    .col(integer_null("duration"))
                    .col(uuid("album_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-tracks-album")
                            .from("tracks", "album_id")
                            .to("albums", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(boolean("explicit"))
                    .col(string_null("isrc"))
                    .col(uuid_null("root_folder_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-tracks-root-folder")
                            .from("tracks", "root_folder_id")
                            .to("root_folders", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(string("status"))
                    .col(string_null("quality_override"))
                    .col(string_null("file_path"))
                    .check(wanted_status_check("ck-tracks-status", "status"))
                    .check(quality_check(
                        "ck-tracks-quality-override",
                        "quality_override",
                    ))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-tracks-album-id")
                    .table("tracks")
                    .col("album_id")
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-tracks-root-folder-id")
                    .table("tracks")
                    .col("root_folder_id")
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("track_artists")
                    .col(uuid("track_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-track-artists-track")
                            .from("track_artists", "track_id")
                            .to("tracks", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(uuid("artist_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-track-artists-artist")
                            .from("track_artists", "artist_id")
                            .to("artists", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(integer("priority").default(0))
                    .primary_key(Index::create().col("track_id").col("artist_id"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-track-artists-artist-id")
                    .table("track_artists")
                    .col("artist_id")
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("track_provider_links")
                    .col(pk_uuid("id"))
                    .col(uuid("track_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-track_provider_links-track_id")
                            .from("track_provider_links", "track_id")
                            .to("tracks", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(string("provider"))
                    .col(string("provider_track_id"))
                    .check(provider_check(
                        "ck-track-provider-links-provider",
                        "provider",
                    ))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-track-provider-links-track-id")
                    .table("track_provider_links")
                    .col("track_id")
                    .to_owned(),
            )
            .await?;

        // Album Artists

        manager
            .create_table(
                Table::create()
                    .table("album_artists")
                    .col(uuid("album_id"))
                    .col(uuid("artist_id"))
                    .col(integer("priority").default(0))
                    .primary_key(Index::create().col("album_id").col("artist_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-album-artists-artist")
                            .from("album_artists", "artist_id")
                            .to("artists", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-album-artists-album")
                            .from("album_artists", "album_id")
                            .to("albums", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-album-artists-artist-id")
                    .table("album_artists")
                    .col("artist_id")
                    .to_owned(),
            )
            .await?;

        // Auth

        manager
            .create_table(
                Table::create()
                    .table("auth_settings")
                    .col(pk_uuid("id"))
                    .col(string("admin_username").default("admin"))
                    .col(string("admin_password_hash").default(""))
                    .col(boolean("must_change_password").default(true))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .col(timestamp_with_time_zone_null("password_changed_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("auth_sessions")
                    .col(pk_uuid("id"))
                    .col(string("session_token_hash"))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .col(timestamp_with_time_zone("expires_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-auth-sessions-session-token-hash")
                    .table("auth_sessions")
                    .col("session_token_hash")
                    .to_owned(),
            )
            .await?;

        // Download

        manager
            .create_table(
                Table::create()
                    .table("download_jobs")
                    .col(pk_uuid("id"))
                    .col(uuid("album_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-download-jobs-album")
                            .from("download_jobs", "album_id")
                            .to("albums", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(uuid_null("track_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-download-jobs-track")
                            .from("download_jobs", "track_id")
                            .to("tracks", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(string("source"))
                    .col(string("quality"))
                    .col(string("status"))
                    .col(integer("total_tracks"))
                    .col(integer("completed_tasks"))
                    .col(string_null("error_message"))
                    .check(provider_with_none_check(
                        "ck-download-jobs-source",
                        "source",
                    ))
                    .check(quality_check("ck-download-jobs-quality", "quality"))
                    .check(download_status_check("ck-download-jobs-status", "status"))
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-download-jobs-album-id")
                    .table("download_jobs")
                    .col("album_id")
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-download-jobs-track-id")
                    .table("download_jobs")
                    .col("track_id")
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx-download-jobs-track-id")
                    .table("download_jobs")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-download-jobs-album-id")
                    .table("download_jobs")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("download_jobs").to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-auth-sessions-session-token-hash")
                    .table("auth_sessions")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("auth_sessions").to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table("auth_settings").to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-album-artists-artist-id")
                    .table("album_artists")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("album_artists").to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-track-provider-links-track-id")
                    .table("track_provider_links")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("track_provider_links").to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-track-artists-artist-id")
                    .table("track_artists")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("track_artists").to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-tracks-root-folder-id")
                    .table("tracks")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-tracks-album-id")
                    .table("tracks")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("tracks").to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-album-match-candidates-album-id")
                    .table("album_match_candidates")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("album_match_candidates").to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-album-provider-links-album-id")
                    .table("album_provider_links")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("album_provider_links").to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-albums-title")
                    .table("albums")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("albums").to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-artist-provider-links-artist-id")
                    .table("artist_provider_links")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-artist-provider-links-external-id")
                    .table("artist_provider_links")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("artist_provider_links").to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-artist-match-candidates-artist-id")
                    .table("artist_match_candidates")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("artist_match_candidates").to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-artists-name")
                    .table("artists")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("artists").to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table("root_folders").to_owned())
            .await?;

        Ok(())
    }
}

fn enum_check(name: &str, column: &str, values: &[&str]) -> Check {
    Check::named(
        Alias::new(name),
        Expr::col(Alias::new(column)).is_in(values.iter().copied()),
    )
}

fn quality_check(name: &str, column: &str) -> Check {
    enum_check(name, column, QUALITY_VALUES)
}

fn wanted_status_check(name: &str, column: &str) -> Check {
    enum_check(name, column, WANTED_STATUS_VALUES)
}

fn provider_check(name: &str, column: &str) -> Check {
    enum_check(name, column, PROVIDER_VALUES)
}

fn provider_with_none_check(name: &str, column: &str) -> Check {
    enum_check(name, column, PROVIDER_VALUES_WITH_NONE)
}

fn match_kind_check(name: &str, column: &str) -> Check {
    enum_check(name, column, MATCH_KIND_VALUES)
}

fn match_status_check(name: &str, column: &str) -> Check {
    enum_check(name, column, MATCH_STATUS_VALUES)
}

fn download_status_check(name: &str, column: &str) -> Check {
    enum_check(name, column, DOWNLOAD_STATUS_VALUES)
}

fn album_type_check(name: &str, column: &str) -> Check {
    enum_check(name, column, ALBUM_TYPE_VALUES)
}

#[cfg(test)]
mod tests {
    use sea_orm_migration::sea_orm::Database;

    use super::*;

    #[tokio::test]
    async fn initial_schema_adds_enum_check_constraints() {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        let manager = SchemaManager::new(&db);

        Migration.up(&manager).await.expect("apply migration");

        let artist_match_sql = table_sql(&db, "artist_match_candidates").await;
        assert!(artist_match_sql.contains(
            r#"CHECK ("left_provider" IN ('tidal', 'deezer', 'music_brainz', 'soulseek'))"#,
        ));
        assert!(artist_match_sql.contains(r#"CHECK ("match_kind" IN ('fuzzy', 'isrc_exact'))"#,));
        assert!(
            artist_match_sql
                .contains(r#"CHECK ("status" IN ('pending', 'accepted', 'dismissed'))"#)
        );

        let albums_sql = table_sql(&db, "albums").await;
        assert!(
            albums_sql.contains(r#"CHECK ("album_type" IN ('album', 'ep', 'single', 'unknown'))"#,)
        );
        assert!(albums_sql.contains(
            r#"CHECK ("wanted_status" IN ('unmonitored', 'wanted', 'in_progress', 'acquired'))"#,
        ));
        assert!(
            albums_sql.contains(
                r#"CHECK ("requested_quality" IN ('low', 'high', 'lossless', 'hi_res'))"#,
            )
        );

        let album_match_sql = table_sql(&db, "album_match_candidates").await;
        assert!(album_match_sql.contains(
            r#"CHECK ("right_provider" IN ('tidal', 'deezer', 'music_brainz', 'soulseek'))"#,
        ));
        assert!(album_match_sql.contains(r#"CHECK ("match_kind" IN ('fuzzy', 'isrc_exact'))"#,));
        assert!(
            album_match_sql.contains(r#"CHECK ("status" IN ('pending', 'accepted', 'dismissed'))"#)
        );

        let tracks_sql = table_sql(&db, "tracks").await;
        assert!(tracks_sql.contains(
            r#"CHECK ("status" IN ('unmonitored', 'wanted', 'in_progress', 'acquired'))"#,
        ));
        assert!(tracks_sql.contains(
            r#"CHECK ("quality_override" IN ('low', 'high', 'lossless', 'hi_res'))"#,
        ));

        let download_jobs_sql = table_sql(&db, "download_jobs").await;
        assert!(download_jobs_sql.contains(
            r#"CHECK ("source" IN ('tidal', 'deezer', 'music_brainz', 'soulseek', 'none'))"#,
        ));
        assert!(
            download_jobs_sql
                .contains(r#"CHECK ("quality" IN ('low', 'high', 'lossless', 'hi_res'))"#,)
        );
        assert!(download_jobs_sql.contains(
            r#"CHECK ("status" IN ('queued', 'resolving', 'downloading', 'completed', 'failed'))"#,
        ));

        let artist_provider_links_sql = table_sql(&db, "artist_provider_links").await;
        assert!(
            artist_provider_links_sql.contains(
                r#"CHECK ("provider" IN ('tidal', 'deezer', 'music_brainz', 'soulseek'))"#,
            )
        );

        let album_provider_links_sql = table_sql(&db, "album_provider_links").await;
        assert!(
            album_provider_links_sql.contains(
                r#"CHECK ("provider" IN ('tidal', 'deezer', 'music_brainz', 'soulseek'))"#,
            )
        );

        let track_provider_links_sql = table_sql(&db, "track_provider_links").await;
        assert!(
            track_provider_links_sql.contains(
                r#"CHECK ("provider" IN ('tidal', 'deezer', 'music_brainz', 'soulseek'))"#,
            )
        );
    }

    #[tokio::test]
    async fn initial_schema_adds_foreign_key_lookup_indexes() {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        let manager = SchemaManager::new(&db);

        Migration.up(&manager).await.expect("apply migration");

        assert!(index_exists(&db, "idx-artist-match-candidates-artist-id").await);
        assert!(index_exists(&db, "idx-artist-provider-links-artist-id").await);
        assert!(index_exists(&db, "idx-album-match-candidates-album-id").await);
        assert!(index_exists(&db, "idx-tracks-album-id").await);
        assert!(index_exists(&db, "idx-tracks-root-folder-id").await);
        assert!(index_exists(&db, "idx-track-artists-artist-id").await);
        assert!(index_exists(&db, "idx-track-provider-links-track-id").await);
        assert!(index_exists(&db, "idx-album-artists-artist-id").await);
        assert!(index_exists(&db, "idx-download-jobs-album-id").await);
        assert!(index_exists(&db, "idx-download-jobs-track-id").await);
    }

    async fn table_sql(db: &sea_orm_migration::sea_orm::DatabaseConnection, table: &str) -> String {
        let stmt = Query::select()
            .column(Alias::new("sql"))
            .from(Alias::new("sqlite_master"))
            .and_where(Expr::col(Alias::new("type")).eq("table"))
            .and_where(Expr::col(Alias::new("name")).eq(table))
            .to_owned();
        let row = db
            .query_one(&stmt)
            .await
            .expect("query sqlite_master")
            .expect("table metadata row");

        row.try_get("", "sql").expect("table sql")
    }

    async fn index_exists(
        db: &sea_orm_migration::sea_orm::DatabaseConnection,
        index: &str,
    ) -> bool {
        let stmt = Query::select()
            .expr(Expr::value(1))
            .from(Alias::new("sqlite_master"))
            .and_where(Expr::col(Alias::new("type")).eq("index"))
            .and_where(Expr::col(Alias::new("name")).eq(index))
            .to_owned();

        db.query_one(&stmt)
            .await
            .expect("query sqlite_master for index")
            .is_some()
    }
}
