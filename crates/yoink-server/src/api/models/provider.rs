use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{db, services::helpers::default_provider_album_url};

/// Provider link info for the UI.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderLink {
    pub provider: db::provider::Provider,
    pub external_id: String,
    pub external_url: Option<String>,
    pub external_name: Option<String>,
}

impl From<db::artist_provider_link::Model> for ProviderLink {
    fn from(value: db::artist_provider_link::Model) -> Self {
        Self {
            provider: value.provider,
            external_id: value.external_id,
            external_url: value.external_url,
            external_name: value.external_name,
        }
    }
}

impl From<db::artist_provider_link::ModelEx> for ProviderLink {
    fn from(value: db::artist_provider_link::ModelEx) -> Self {
        db::artist_provider_link::Model::from(value).into()
    }
}

impl From<db::album_provider_link::Model> for ProviderLink {
    fn from(value: db::album_provider_link::Model) -> Self {
        Self {
            provider: value.provider,
            external_url: value.external_url.or_else(|| {
                default_provider_album_url(&value.provider.to_string(), &value.provider_album_id)
            }),
            external_name: value.external_name,
            external_id: value.provider_album_id,
        }
    }
}
