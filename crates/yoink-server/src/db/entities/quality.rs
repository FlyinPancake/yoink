use std::str::FromStr;

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Copy,
    Serialize,
    Deserialize,
    ToSchema,
    EnumIter,
    DeriveActiveEnum,
    clap::ValueEnum,
)]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    enum_name = "quality",
    rename_all = "snake_case"
)]
#[value(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Quality {
    #[value(alias = "low")]
    Low,
    #[value(alias = "high")]
    High,
    #[value(alias = "lossless")]
    Lossless,
    #[value(
        alias = "hires",
        alias = "hi-res",
        alias = "hi_res",
        alias = "HIRES",
        alias = "HI-RES"
    )]
    HiRes,
}

impl FromStr for Quality {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(&format!("\"{}\"", s))
            .map_err(|_| format!("Invalid quality value: '{}'.", s))
    }
}

impl Quality {
    pub fn as_str(&self) -> &str {
        match self {
            Quality::Lossless => "LOSSLESS",
            Quality::HiRes => "HI_RES_LOSSLESS",
            Quality::High => "HIGH",
            Quality::Low => "LOW",
        }
    }
}

impl std::fmt::Display for Quality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
