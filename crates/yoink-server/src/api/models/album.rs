use chrono::{DateTime, Utc};
use sea_orm::ActiveEnum;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db;

use super::Quality;

/// Album status indicating what the download system should do with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WantedStatus {
    /// Not wanted — either not monitored or explicitly skipped.
    Unwanted,
    /// Wanted — monitored and awaiting download.
    Wanted,
    /// Download is in progress.
    InProgress,
    /// Fully acquired — all monitored tracks are on disk.
    Acquired,
}

/// Core album data as stored in the database.
/// Relation data (artists, tracks, provider links) is returned separately
/// in API responses — not embedded here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Album {
    pub id: Uuid,
    pub title: String,
    pub album_type: Option<String>,
    pub release_date: Option<String>,
    pub cover_url: Option<String>,
    pub explicit: bool,
    pub monitored: bool,
    pub wanted_status: WantedStatus,
    #[serde(default)]
    pub quality_override: Option<Quality>,
    pub created_at: DateTime<Utc>,
}

impl From<db::wanted_status::WantedStatus> for WantedStatus {
    fn from(value: db::wanted_status::WantedStatus) -> Self {
        match value {
            db::wanted_status::WantedStatus::Unmonitored => Self::Unwanted,
            db::wanted_status::WantedStatus::Wanted => Self::Wanted,
            db::wanted_status::WantedStatus::InProgress => Self::InProgress,
            db::wanted_status::WantedStatus::Acquired => Self::Acquired,
        }
    }
}

impl From<db::album::Model> for Album {
    fn from(value: db::album::Model) -> Self {
        Self {
            id: value.id,
            title: value.title,
            album_type: Some(value.album_type.to_value()),
            release_date: value.release_date.map(|d| d.to_string()),
            cover_url: value.cover_url.map(|u| u.to_string()),
            explicit: value.explicit,
            monitored: value.wanted_status != db::wanted_status::WantedStatus::Unmonitored,
            wanted_status: value.wanted_status.into(),
            quality_override: value.requested_quality.map(Into::into),
            created_at: value.created_at,
        }
    }
}

impl From<db::album::ModelEx> for Album {
    fn from(value: db::album::ModelEx) -> Self {
        db::album::Model::from(value).into()
    }
}
