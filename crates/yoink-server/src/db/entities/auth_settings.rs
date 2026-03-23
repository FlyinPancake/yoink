use sea_orm::{ActiveValue::Set, entity::prelude::*};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "auth_settings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    #[sea_orm(default_value = "admin")]
    pub admin_username: String,
    #[sea_orm(default_value = "")]
    pub admin_password_hash: String,
    #[sea_orm(default_value = "true")]
    pub must_change_password: bool,
    pub created_at: DateTimeUtc,
    pub modified_at: DateTimeUtc,
    pub password_changed_at: Option<DateTimeUtc>,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            // Only one row will ever be inserted, so we can just set the ID to 1 so if we try to insert another row, it will fail with a unique constraint violation
            id: Set(Uuid::nil()),
            ..ActiveModelTrait::default()
        }
    }

    /// Will be triggered before insert / update
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now = chrono::Utc::now();
        self.modified_at = Set(now);
        if insert {
            self.created_at = Set(now);
        }

        if self.admin_password_hash.is_set() && (insert || self.password_changed_at.is_not_set()) {
            self.password_changed_at = Set(Some(now));
        }

        Ok(self)
    }
}

pub enum SettingsResult {
    Bootstrapped(Model),
    Existing(Model),
}

impl SettingsResult {
    pub fn into_model(self) -> Model {
        match self {
            SettingsResult::Bootstrapped(model) => model,
            SettingsResult::Existing(model) => model,
        }
    }
}

impl Entity {
    pub async fn get_settings<C>(db: &C) -> Result<SettingsResult, DbErr>
    where
        C: ConnectionTrait,
    {
        // There should only ever be one row in this table, so we can just get the first one
        match Self::find().one(db).await? {
            Some(settings) => Ok(SettingsResult::Existing(settings)),
            None => {
                // If there are no settings, we need to create the default settings row
                let new_settings = ActiveModel {
                    ..Default::default()
                };
                let new_settings = new_settings.insert(db).await?;
                super::auth_session::Entity::delete_many().exec(db).await?;
                Ok(SettingsResult::Bootstrapped(new_settings))
            }
        }
    }
}
