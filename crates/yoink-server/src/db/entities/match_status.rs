use sea_orm::entity::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(
    rs_type = "String",
    db_type = "Text",
    rename_all = "snake_case",
    enum_name = "match_status"
)]
pub enum MatchStatus {
    Pending,
    Accepted,
    Dismissed,
}
