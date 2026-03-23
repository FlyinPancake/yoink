use sea_orm::{ActiveValue::Set, entity::prelude::*};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "auth_sessions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub session_token_hash: String,
    pub created_at: DateTimeUtc,
    pub modified_at: DateTimeUtc,
    pub expires_at: DateTimeUtc,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: Set(Uuid::now_v7()),
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
        Ok(self)
    }
}

impl Entity {
    pub async fn delete_expired_sessions<C>(db: &C) -> Result<(), DbErr>
    where
        C: ConnectionTrait,
    {
        Self::delete_many()
            .filter(Column::ExpiresAt.lte(chrono::Utc::now()))
            .exec(db)
            .await?;
        Ok(())
    }

    pub async fn find_by_session_token_hash<C>(
        db: &C,
        session_token_hash: &str,
    ) -> Result<Option<Model>, DbErr>
    where
        C: ConnectionTrait,
    {
        Self::find()
            .filter(Column::SessionTokenHash.eq(session_token_hash))
            .one(db)
            .await
    }
}
