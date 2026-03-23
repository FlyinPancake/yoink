use sea_orm::DeriveValueType;
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, DeriveValueType)]
#[sea_orm(value_type = "String", column_type = "Text")]
pub struct DbUrl(pub Url);

impl std::str::FromStr for DbUrl {
    type Err = url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Url::parse(s)?))
    }
}

impl std::fmt::Display for DbUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<DbUrl> for Url {
    fn from(value: DbUrl) -> Self {
        value.0
    }
}
