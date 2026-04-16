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
    #[sea_orm(default_value_t = true)]
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

impl Entity {
    pub async fn get<C>(db: &C) -> Result<Option<Model>, DbErr>
    where
        C: ConnectionTrait,
    {
        Self::find().one(db).await
    }
}
