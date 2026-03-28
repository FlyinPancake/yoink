use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, EnumIter, DeriveActiveEnum,
)]
#[sea_orm(
    rs_type = "String",
    db_type = "Text",
    rename_all = "snake_case",
    enum_name = "wanted_status"
)]
#[serde(rename_all = "snake_case")]
pub enum WantedStatus {
    Unmonitored,
    Wanted,
    InProgress,
    Acquired,
}
