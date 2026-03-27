use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    ToSchema,
    EnumIter,
    DeriveActiveEnum,
)]
#[sea_orm(
    rs_type = "String",
    db_type = "Text",
    rename_all = "snake_case",
    enum_name = "match_kind"
)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Fuzzy,
    IsrcExact,
}
