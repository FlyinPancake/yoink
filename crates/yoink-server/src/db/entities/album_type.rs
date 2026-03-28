use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema,
)]
#[sea_orm(
    rs_type = "String",
    db_type = "Enum",
    enum_name = "album_type",
    rename_all = "snake_case"
)]
#[serde(rename_all = "snake_case")]
pub enum AlbumType {
    Album,
    EP,
    Single,
    Unknown,
}

impl AlbumType {
    /// Parse an album type string from any provider (case-insensitive).
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "album" => Self::Album,
            "ep" | "e_p" => Self::EP,
            "single" => Self::Single,
            _ => Self::Unknown,
        }
    }
}
