use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MonitoredArtist {
    pub id: Uuid,
    pub name: String,
    pub image_url: Option<String>,
    pub bio: Option<String>,
    /// Whether this artist is fully monitored (discography synced from providers).
    /// `false` = lightweight artist (only explicitly-added albums, no auto-sync).
    pub monitored: bool,
    pub created_at: DateTime<Utc>,
}

/// An artist image option from a linked provider.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArtistImageOption {
    pub provider: String,
    pub image_url: String,
}

impl From<db::artist::Model> for MonitoredArtist {
    fn from(value: db::artist::Model) -> Self {
        Self {
            id: value.id,
            name: value.name,
            image_url: value.image_url,
            bio: value.bio,
            monitored: value.monitored,
            created_at: value.created_at,
        }
    }
}

impl From<db::artist::ModelEx> for MonitoredArtist {
    fn from(value: db::artist::ModelEx) -> Self {
        db::artist::Model::from(value).into()
    }
}
