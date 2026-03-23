use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(
    rs_type = "String",
    db_type = "Enum",
    enum_name = "download_status",
    rename_all = "snake_case"
)]
pub enum DownloadStatus {
    Queued,
    Resolving,
    Downloading,
    Completed,
    Failed,
}

impl From<DownloadStatus> for yoink_shared::DownloadStatus {
    fn from(value: DownloadStatus) -> Self {
        match value {
            DownloadStatus::Queued => Self::Queued,
            DownloadStatus::Resolving => Self::Resolving,
            DownloadStatus::Downloading => Self::Downloading,
            DownloadStatus::Completed => Self::Completed,
            DownloadStatus::Failed => Self::Failed,
        }
    }
}

impl From<yoink_shared::DownloadStatus> for DownloadStatus {
    fn from(value: yoink_shared::DownloadStatus) -> Self {
        match value {
            yoink_shared::DownloadStatus::Queued => Self::Queued,
            yoink_shared::DownloadStatus::Resolving => Self::Resolving,
            yoink_shared::DownloadStatus::Downloading => Self::Downloading,
            yoink_shared::DownloadStatus::Completed => Self::Completed,
            yoink_shared::DownloadStatus::Failed => Self::Failed,
        }
    }
}

impl DownloadStatus {
    pub fn in_progress(&self) -> bool {
        matches!(
            self,
            DownloadStatus::Queued | DownloadStatus::Resolving | DownloadStatus::Downloading
        )
    }
}
