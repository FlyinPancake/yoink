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
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[sea_orm(
    rs_type = "String",
    db_type = "Enum",
    enum_name = "provider",
    rename_all = "snake_case"
)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Tidal,
    Deezer,
    MusicBrainz,
    Soulseek,
    None,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_value())
    }
}

impl std::str::FromStr for Provider {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tidal" => Ok(Provider::Tidal),
            "deezer" => Ok(Provider::Deezer),
            "musicbrainz" | "music_brainz" | "music-brainz" => Ok(Provider::MusicBrainz),
            "soulseek" => Ok(Provider::Soulseek),
            "none" => Ok(Provider::None),
            _ => Err(()),
        }
    }
}
