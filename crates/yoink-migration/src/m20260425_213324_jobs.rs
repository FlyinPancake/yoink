use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Rename the existing download_jobs table to legacy_download_jobs, next version will drop this table, allows for going back to the old version if needed
        manager
            .rename_table(
                Table::rename()
                    .table("download_jobs", "legacy_download_jobs")
                    .to_owned(),
            )
            .await?;

        // Create the new jobs table, which will be used for all types of jobs, not just downloads
        manager
            .create_table(
                Table::create()
                    .table("jobs")
                    .col(pk_uuid("id"))
                    .col(string_len("job_kind", 20))
                    .col(json("data"))
                    .col(string_len("status", 20))
                    .col(string("deduplication_key").unique_key())
                    .col(timestamp_with_time_zone("created_at"))
                    .col(timestamp_with_time_zone("modified_at"))
                    .col(integer("attempts"))
                    .col(integer("max_attempts"))
                    // progress 0 -> 1
                    .col(float("progress"))
                    .col(string_null("error_message"))
                    .col(timestamp_with_time_zone_null("started_at"))
                    .col(timestamp_with_time_zone_null("finished_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-jobs-job-type")
                    .table("jobs")
                    .col("job_type")
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-jobs-status")
                    .table("jobs")
                    .col("status")
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("jobs").to_owned())
            .await?;

        if manager.has_table("legacy_download_jobs").await? {
            manager
                .rename_table(
                    Table::rename()
                        .table("legacy_download_jobs", "download_jobs")
                        .to_owned(),
                )
                .await?;
        } else {
            // If the legacy_download_jobs table doesn't exist, we can't rename it back, so we just log a warning
            // TODO maybe log this more prominently, but rolling back is not really a common operation
            eprintln!(
                "Warning: legacy_download_jobs table does not exist, cannot rename back to download_jobs"
            );
        }

        Ok(())
    }
}
