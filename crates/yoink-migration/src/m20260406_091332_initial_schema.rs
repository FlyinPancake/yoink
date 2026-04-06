use sea_orm_migration::{prelude::*, schema::*};

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

        // Aritst

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
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
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
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
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
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
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
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
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
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
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
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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
            .drop_table(Table::drop().table("album_artists").to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table("track_provider_links").to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table("track_artists").to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table("tracks").to_owned())
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
                    .name("idx-artist-provider-links-external-id")
                    .table("artist_provider_links")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table("artist_provider_links").to_owned())
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
