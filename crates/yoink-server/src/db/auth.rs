use chrono::{DateTime, Utc};
use sqlx::{Executor, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;
use veil::Redact;

#[derive(Clone, Redact)]
pub(crate) struct AuthSettingsRecord {
    pub(crate) admin_username: String,
    #[redact]
    pub(crate) password_hash: String,
    pub(crate) must_change_password: bool,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) password_changed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Redact)]
pub(crate) struct AuthSessionRecord {
    pub(crate) id: Uuid,
    #[redact]
    pub(crate) session_token_hash: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) last_seen_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
}

pub(crate) async fn load_auth_settings(
    pool: &SqlitePool,
) -> Result<Option<AuthSettingsRecord>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT
            admin_username,
            password_hash,
            must_change_password as "must_change_password!: bool",
            created_at as "created_at!: chrono::DateTime<chrono::Utc>",
            updated_at as "updated_at!: chrono::DateTime<chrono::Utc>",
            password_changed_at as "password_changed_at: chrono::DateTime<chrono::Utc>"
        FROM auth_settings
        WHERE singleton = 1"#
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| AuthSettingsRecord {
        admin_username: row.admin_username,
        password_hash: row.password_hash,
        must_change_password: row.must_change_password,
        created_at: row.created_at,
        updated_at: row.updated_at,
        password_changed_at: row.password_changed_at,
    }))
}

pub(crate) async fn insert_auth_settings(
    pool: &SqlitePool,
    settings: &AuthSettingsRecord,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO auth_settings (
            singleton, admin_username, password_hash, must_change_password,
            created_at, updated_at, password_changed_at
        ) VALUES (1, $1, $2, $3, $4, $5, $6)"#,
        settings.admin_username,
        settings.password_hash,
        settings.must_change_password,
        settings.created_at,
        settings.updated_at,
        settings.password_changed_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn update_auth_settings_tx(
    tx: &mut Transaction<'_, Sqlite>,
    admin_username: &str,
    password_hash: &str,
    must_change_password: bool,
    updated_at: DateTime<Utc>,
    password_changed_at: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    update_auth_settings_with_executor(
        &mut **tx,
        admin_username,
        password_hash,
        must_change_password,
        updated_at,
        password_changed_at,
    )
    .await
}

async fn update_auth_settings_with_executor<'e, E>(
    executor: E,
    admin_username: &str,
    password_hash: &str,
    must_change_password: bool,
    updated_at: DateTime<Utc>,
    password_changed_at: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error>
where
    E: Executor<'e, Database = Sqlite>,
{
    let result = sqlx::query!(
        r#"UPDATE auth_settings
        SET admin_username = $1,
            password_hash = $2,
            must_change_password = $3,
            updated_at = $4,
            password_changed_at = $5
        WHERE singleton = 1"#,
        admin_username,
        password_hash,
        must_change_password,
        updated_at,
        password_changed_at,
    )
    .execute(executor)
    .await?;
    if result.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }

    Ok(())
}

pub(crate) async fn insert_auth_session(
    pool: &SqlitePool,
    session: &AuthSessionRecord,
) -> Result<(), sqlx::Error> {
    insert_auth_session_with_executor(pool, session).await
}

pub(crate) async fn insert_auth_session_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session: &AuthSessionRecord,
) -> Result<(), sqlx::Error> {
    insert_auth_session_with_executor(&mut **tx, session).await
}

async fn insert_auth_session_with_executor<'e, E>(
    executor: E,
    session: &AuthSessionRecord,
) -> Result<(), sqlx::Error>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query!(
        r#"INSERT INTO auth_sessions (
            id, session_token_hash, created_at, last_seen_at, expires_at
        ) VALUES ($1, $2, $3, $4, $5)"#,
        session.id,
        session.session_token_hash,
        session.created_at,
        session.last_seen_at,
        session.expires_at,
    )
    .execute(executor)
    .await?;
    Ok(())
}

pub(crate) async fn load_auth_session_by_hash(
    pool: &SqlitePool,
    session_token_hash: &str,
) -> Result<Option<AuthSessionRecord>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT
            id as "id!: Uuid",
            session_token_hash,
            created_at as "created_at!: chrono::DateTime<chrono::Utc>",
            last_seen_at as "last_seen_at!: chrono::DateTime<chrono::Utc>",
            expires_at as "expires_at!: chrono::DateTime<chrono::Utc>"
        FROM auth_sessions
        WHERE session_token_hash = $1"#,
        session_token_hash,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| AuthSessionRecord {
        id: row.id,
        session_token_hash: row.session_token_hash,
        created_at: row.created_at,
        last_seen_at: row.last_seen_at,
        expires_at: row.expires_at,
    }))
}

pub(crate) async fn touch_auth_session(
    pool: &SqlitePool,
    session_id: Uuid,
    last_seen_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE auth_sessions SET last_seen_at = $1, expires_at = $2 WHERE id = $3",
        last_seen_at,
        expires_at,
        session_id,
    )
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }

    Ok(())
}

pub(crate) async fn delete_auth_session(
    pool: &SqlitePool,
    session_id: Uuid,
) -> Result<(), sqlx::Error> {
    let result = sqlx::query!("DELETE FROM auth_sessions WHERE id = $1", session_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }

    Ok(())
}

pub(crate) async fn delete_all_auth_sessions_tx(
    tx: &mut Transaction<'_, Sqlite>,
) -> Result<u64, sqlx::Error> {
    delete_all_auth_sessions_with_executor(&mut **tx).await
}

async fn delete_all_auth_sessions_with_executor<'e, E>(executor: E) -> Result<u64, sqlx::Error>
where
    E: Executor<'e, Database = Sqlite>,
{
    let result = sqlx::query!("DELETE FROM auth_sessions")
        .execute(executor)
        .await?;
    Ok(result.rows_affected())
}

pub(crate) async fn delete_expired_auth_sessions(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM auth_sessions WHERE expires_at <= $1", now)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use crate::test_helpers::test_db;

    use super::*;

    #[tokio::test]
    async fn auth_settings_round_trip() {
        let pool = test_db().await;
        let now = Utc::now();

        let settings = AuthSettingsRecord {
            admin_username: "admin".to_string(),
            password_hash: "hash".to_string(),
            must_change_password: true,
            created_at: now,
            updated_at: now,
            password_changed_at: None,
        };

        insert_auth_settings(&pool, &settings).await.unwrap();
        let loaded = load_auth_settings(&pool).await.unwrap().unwrap();

        assert_eq!(loaded.admin_username, "admin");
        assert!(loaded.must_change_password);
    }

    #[tokio::test]
    async fn auth_session_round_trip_and_expiry_cleanup() {
        let pool = test_db().await;
        let now = Utc::now();

        let active = AuthSessionRecord {
            id: Uuid::now_v7(),
            session_token_hash: "active".to_string(),
            created_at: now,
            last_seen_at: now,
            expires_at: now + Duration::hours(1),
        };
        let expired = AuthSessionRecord {
            id: Uuid::now_v7(),
            session_token_hash: "expired".to_string(),
            created_at: now,
            last_seen_at: now,
            expires_at: now - Duration::hours(1),
        };

        insert_auth_session(&pool, &active).await.unwrap();
        insert_auth_session(&pool, &expired).await.unwrap();

        let loaded = load_auth_session_by_hash(&pool, "active").await.unwrap();
        assert_eq!(loaded.unwrap().id, active.id);

        let deleted = delete_expired_auth_sessions(&pool, now).await.unwrap();
        assert_eq!(deleted, 1);
        assert!(
            load_auth_session_by_hash(&pool, "expired")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn delete_all_auth_sessions_clears_records() {
        let pool = test_db().await;
        let now = Utc::now();

        let session = AuthSessionRecord {
            id: Uuid::now_v7(),
            session_token_hash: "active".to_string(),
            created_at: now,
            last_seen_at: now,
            expires_at: now + Duration::hours(1),
        };

        insert_auth_session(&pool, &session).await.unwrap();

        let deleted = delete_all_auth_sessions_with_executor(&pool).await.unwrap();
        assert_eq!(deleted, 1);
        assert!(
            load_auth_session_by_hash(&pool, "active")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn update_auth_settings_returns_row_not_found_when_missing() {
        let pool = test_db().await;
        let now = Utc::now();
        let mut tx = pool.begin().await.unwrap();

        let err = update_auth_settings_tx(&mut tx, "admin", "hash", false, now, Some(now))
            .await
            .unwrap_err();

        assert!(matches!(err, sqlx::Error::RowNotFound));
    }

    #[tokio::test]
    async fn touch_auth_session_returns_row_not_found_when_missing() {
        let pool = test_db().await;
        let now = Utc::now();

        let err = touch_auth_session(&pool, Uuid::now_v7(), now, now + Duration::hours(1))
            .await
            .unwrap_err();

        assert!(matches!(err, sqlx::Error::RowNotFound));
    }

    #[tokio::test]
    async fn delete_auth_session_returns_row_not_found_when_missing() {
        let pool = test_db().await;

        let err = delete_auth_session(&pool, Uuid::now_v7())
            .await
            .unwrap_err();

        assert!(matches!(err, sqlx::Error::RowNotFound));
    }

    #[test]
    fn auth_records_redact_sensitive_fields_in_debug_output() {
        let now = Utc::now();
        let settings = AuthSettingsRecord {
            admin_username: "admin".to_string(),
            password_hash: "super-secret-hash".to_string(),
            must_change_password: true,
            created_at: now,
            updated_at: now,
            password_changed_at: None,
        };
        let session = AuthSessionRecord {
            id: Uuid::nil(),
            session_token_hash: "session-token-hash".to_string(),
            created_at: now,
            last_seen_at: now,
            expires_at: now,
        };

        let settings_debug = format!("{settings:?}");
        let session_debug = format!("{session:?}");

        assert!(settings_debug.contains("admin"));
        assert!(!settings_debug.contains("super-secret-hash"));
        assert!(!session_debug.contains("session-token-hash"));
        assert!(session_debug.contains(&Uuid::nil().to_string()));
    }
}
