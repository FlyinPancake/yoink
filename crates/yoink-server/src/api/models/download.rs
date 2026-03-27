use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::download_status::DownloadStatus;

use super::Quality;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadJobKind {
    Album,
    Track,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DownloadJob {
    pub id: Uuid,
    pub kind: DownloadJobKind,
    pub album_id: Uuid,
    pub track_id: Option<Uuid>,
    pub source: String,
    pub album_title: String,
    pub track_title: Option<String>,
    pub artist_name: String,
    pub status: DownloadStatus,
    pub quality: Quality,
    pub total_tracks: i32,
    pub completed_tracks: i32,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
