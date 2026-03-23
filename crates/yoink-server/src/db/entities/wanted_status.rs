use sea_orm::entity::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(
    rs_type = "String",
    db_type = "Text",
    rename_all = "snake_case",
    enum_name = "wanted_status"
)]
pub enum WantedStatus {
    Unmonitored,
    Wanted,
    InProgress,
    Acquired,
}
