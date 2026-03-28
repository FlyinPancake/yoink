use chrono::{DateTime, Utc};
use sea_orm::ActiveEnum;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db;

use super::{Quality, WantedStatus};

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
