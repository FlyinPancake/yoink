use std::str::FromStr;

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Clone, Debug, PartialEq, Eq, Copy, Serialize, Deserialize, ToSchema, EnumIter, DeriveActiveEnum,
)]
#[sea_orm(
    rs_type = "String",
    db_type = "Enum",
    enum_name = "quality",
    rename_all = "snake_case"
)]
pub enum Quality {
    Low,
    High,
    Lossless,
    HiRes,
}

impl FromStr for Quality {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "low" => Ok(Quality::Low),
            "high" => Ok(Quality::High),
            "lossless" => Ok(Quality::Lossless),
            "hires" | "hi-res" | "hi_res" => Ok(Quality::HiRes),
            _ => Err(format!("Invalid quality: {}", s)),
        }
    }
}

impl From<Quality> for yoink_shared::Quality {
    fn from(value: Quality) -> Self {
        match value {
            Quality::Low => Self::Low,
            Quality::High => Self::High,
            Quality::Lossless => Self::Lossless,
            Quality::HiRes => Self::HiRes,
        }
    }
}

impl From<yoink_shared::Quality> for Quality {
    fn from(value: yoink_shared::Quality) -> Self {
        match value {
            yoink_shared::Quality::Low => Self::Low,
            yoink_shared::Quality::High => Self::High,
            yoink_shared::Quality::Lossless => Self::Lossless,
            yoink_shared::Quality::HiRes => Self::HiRes,
        }
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
