use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tracing::warn;
use uuid::Uuid;

use crate::{
    app_config::AuthConfig,
    db::{
        AuthSessionRecord, AuthSettingsRecord, delete_all_auth_sessions_tx, delete_auth_session,
        delete_expired_auth_sessions, insert_auth_session, insert_auth_session_tx,
        insert_auth_settings, load_auth_session_by_hash, load_auth_settings, touch_auth_session,
        update_auth_settings_tx,
    },
    error::{AppError, AppResult},
};

const DEFAULT_BOOTSTRAP_USERNAME: &str = "admin";

#[derive(Debug, Clone)]
pub(crate) struct AuthenticatedSession {
    pub(crate) username: String,
    pub(crate) must_change_password: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct LoginOutcome {
    pub(crate) cookie_value: String,
    pub(crate) must_change_password: bool,
}

#[derive(Clone)]
pub(crate) struct AuthService {
    enabled: bool,
    session_secret: String,
    db: SqlitePool,
}

impl AuthService {
    pub(crate) async fn new(config: AuthConfig, db: SqlitePool) -> AppResult<Self> {
        let service = Self {
            enabled: config.enabled,
            session_secret: config.session_secret.clone(),
            db,
        };

        if !service.enabled {
            return Ok(service);
        }

        service.bootstrap_if_needed(&config).await?;
        Ok(service)
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) async fn login(
        &self,
        username: &str,
        password: &str,
    ) -> AppResult<Option<LoginOutcome>> {
        if !self.enabled {
            return Ok(Some(LoginOutcome {
                cookie_value: String::new(),
                must_change_password: false,
            }));
        }

        delete_expired_auth_sessions(&self.db, Utc::now()).await?;
        let settings = self
            .load_settings()
            .await?
            .ok_or_else(|| AppError::unavailable("auth", "auth settings missing"))?;

        if settings.admin_username != username.trim() {
            return Ok(None);
        }

        if !self.verify_password(password, &settings.password_hash)? {
            return Ok(None);
        }

        let (session, outcome) = self.build_login_session(settings.must_change_password);
        insert_auth_session(&self.db, &session).await?;

        Ok(Some(outcome))
    }

    pub(crate) async fn authenticate_request(
        &self,
        cookie_value: Option<&str>,
        rolling: bool,
    ) -> AppResult<Option<AuthenticatedSession>> {
        if !self.enabled {
            return Ok(Some(AuthenticatedSession {
                username: DEFAULT_BOOTSTRAP_USERNAME.to_string(),
                must_change_password: false,
            }));
        }

        let Some(raw_token) = cookie_value.and_then(|value| self.verify_signed_cookie(value))
        else {
            return Ok(None);
        };

        delete_expired_auth_sessions(&self.db, Utc::now()).await?;

        let settings = self
            .load_settings()
            .await?
            .ok_or_else(|| AppError::unavailable("auth", "auth settings missing"))?;
        let token_hash = hash_value(&raw_token);
        let Some(session) = load_auth_session_by_hash(&self.db, &token_hash).await? else {
            return Ok(None);
        };

        if session.expires_at <= Utc::now() {
            delete_auth_session(&self.db, session.id).await?;
            return Ok(None);
        }

        if rolling {
            let now = Utc::now();
            touch_auth_session(&self.db, session.id, now, now + Duration::hours(24)).await?;
        }

        Ok(Some(AuthenticatedSession {
            username: settings.admin_username,
            must_change_password: settings.must_change_password,
        }))
    }

    pub(crate) async fn logout(&self, cookie_value: Option<&str>) -> AppResult<()> {
        if !self.enabled {
            return Ok(());
        }

        delete_expired_auth_sessions(&self.db, Utc::now()).await?;
        if let Some(raw_token) = cookie_value.and_then(|value| self.verify_signed_cookie(value))
            && let Some(session) =
                load_auth_session_by_hash(&self.db, &hash_value(&raw_token)).await?
        {
            delete_auth_session(&self.db, session.id).await?;
        }

        Ok(())
    }

    pub(crate) async fn update_credentials(
        &self,
        username: &str,
        new_password: &str,
    ) -> AppResult<LoginOutcome> {
        let settings = self
            .load_settings()
            .await?
            .ok_or_else(|| AppError::unavailable("auth", "auth settings missing"))?;

        let trimmed_username = username.trim();
        if trimmed_username.is_empty() {
            return Err(AppError::validation(
                Some("username"),
                "username cannot be empty",
            ));
        }
        if new_password.trim().is_empty() {
            return Err(AppError::validation(
                Some("new_password"),
                "password cannot be empty",
            ));
        }

        let now = Utc::now();
        let password_hash = hash_password(new_password)?;
        let (session, outcome) = self.build_login_session(false);
        let mut tx = self.db.begin().await?;

        update_auth_settings_tx(
            &mut tx,
            trimmed_username,
            &password_hash,
            false,
            now,
            Some(now),
        )
        .await?;
        delete_all_auth_sessions_tx(&mut tx).await?;
        insert_auth_session_tx(&mut tx, &session).await?;
        tx.commit().await?;

        if settings.must_change_password {
            warn!(
                username = trimmed_username,
                "Bootstrap credentials replaced; all sessions revoked"
            );
        }

        Ok(outcome)
    }

    pub(crate) async fn verify_current_password(&self, password: &str) -> AppResult<bool> {
        let settings = self
            .load_settings()
            .await?
            .ok_or_else(|| AppError::unavailable("auth", "auth settings missing"))?;
        self.verify_password(password, &settings.password_hash)
    }

    async fn bootstrap_if_needed(&self, config: &AuthConfig) -> AppResult<()> {
        if let Some(settings) = self.load_settings().await? {
            if settings.must_change_password {
                self.rotate_temporary_password(&settings.admin_username)
                    .await?;
                return Ok(());
            }
            delete_expired_auth_sessions(&self.db, Utc::now()).await?;
            return Ok(());
        }

        let now = Utc::now();
        match (
            config.init_admin_username.as_deref(),
            config.init_admin_password.as_deref(),
        ) {
            (Some(username), Some(password)) => {
                let settings = AuthSettingsRecord {
                    admin_username: username.to_string(),
                    password_hash: hash_password(password)?,
                    must_change_password: false,
                    created_at: now,
                    updated_at: now,
                    password_changed_at: Some(now),
                };
                insert_auth_settings(&self.db, &settings).await?;
            }
            _ => {
                let temp_password = random_token(18);
                let settings = AuthSettingsRecord {
                    admin_username: DEFAULT_BOOTSTRAP_USERNAME.to_string(),
                    password_hash: hash_password(&temp_password)?,
                    must_change_password: true,
                    created_at: now,
                    updated_at: now,
                    password_changed_at: None,
                };
                insert_auth_settings(&self.db, &settings).await?;
                warn!(
                    username = DEFAULT_BOOTSTRAP_USERNAME,
                    temporary_password = %temp_password,
                    "Generated temporary bootstrap admin password. Log in and change it immediately."
                );
            }
        }

        Ok(())
    }

    async fn rotate_temporary_password(&self, username: &str) -> AppResult<()> {
        let temp_password = random_token(18);
        let now = Utc::now();
        let mut tx = self.db.begin().await?;
        update_auth_settings_tx(
            &mut tx,
            username,
            &hash_password(&temp_password)?,
            true,
            now,
            None,
        )
        .await?;
        delete_all_auth_sessions_tx(&mut tx).await?;
        tx.commit().await?;
        warn!(
            username,
            temporary_password = ?temp_password,
            "Previous bootstrap setup was not completed. Generated a new temporary admin password."
        );
        Ok(())
    }

    async fn load_settings(&self) -> AppResult<Option<AuthSettingsRecord>> {
        load_auth_settings(&self.db).await.map_err(Into::into)
    }

    fn sign_cookie_value(&self, raw_token: &str) -> String {
        format!("{raw_token}.{}", self.cookie_signature(raw_token))
    }

    fn verify_signed_cookie(&self, value: &str) -> Option<String> {
        let (raw_token, signature) = value.rsplit_once('.')?;
        if self.cookie_signature(raw_token) == signature {
            Some(raw_token.to_string())
        } else {
            None
        }
    }

    fn cookie_signature(&self, raw_token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.session_secret.as_bytes());
        hasher.update(b":");
        hasher.update(raw_token.as_bytes());
        let digest = hasher.finalize();
        hex_encode(&digest)
    }

    fn verify_password(&self, password: &str, password_hash: &str) -> AppResult<bool> {
        let parsed = PasswordHash::new(password_hash).map_err(|err| {
            AppError::unavailable("auth", format!("invalid password hash: {err}"))
        })?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }

    fn build_login_session(&self, must_change_password: bool) -> (AuthSessionRecord, LoginOutcome) {
        let raw_token = random_token(32);
        let now = Utc::now();
        let session = AuthSessionRecord {
            id: Uuid::now_v7(),
            session_token_hash: hash_value(&raw_token),
            created_at: now,
            last_seen_at: now,
            expires_at: now + Duration::hours(24),
        };

        (
            session,
            LoginOutcome {
                cookie_value: self.sign_cookie_value(&raw_token),
                must_change_password,
            },
        )
    }
}

fn hash_password(password: &str) -> AppResult<String> {
    let salt_bytes: [u8; 16] = rand::random();
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|err| AppError::unavailable("auth", format!("invalid generated salt: {err}")))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| AppError::unavailable("auth", format!("password hashing failed: {err}")))
}

fn random_token(num_bytes: usize) -> String {
    let bytes: Vec<u8> = (0..num_bytes).map(|_| rand::random::<u8>()).collect();
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_value(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use crate::{
        app_config::AuthConfig,
        db::{
            AuthSessionRecord, insert_auth_session, load_auth_session_by_hash, load_auth_settings,
        },
        test_helpers::test_db,
    };

    use super::*;

    #[tokio::test]
    async fn bootstraps_from_init_env_password() {
        let pool = test_db().await;
        let service = AuthService::new(
            AuthConfig {
                enabled: true,
                session_secret: "secret".to_string(),
                init_admin_username: Some("root".to_string()),
                init_admin_password: Some("password123".to_string()),
            },
            pool.clone(),
        )
        .await
        .unwrap();

        assert!(service.enabled());
        let settings = load_auth_settings(&pool).await.unwrap().unwrap();
        assert_eq!(settings.admin_username, "root");
        assert!(!settings.must_change_password);
    }

    #[tokio::test]
    async fn bootstraps_temp_password_when_no_init_credentials() {
        let pool = test_db().await;
        AuthService::new(
            AuthConfig {
                enabled: true,
                session_secret: "secret".to_string(),
                init_admin_username: None,
                init_admin_password: None,
            },
            pool.clone(),
        )
        .await
        .unwrap();

        let settings = load_auth_settings(&pool).await.unwrap().unwrap();
        assert_eq!(settings.admin_username, DEFAULT_BOOTSTRAP_USERNAME);
        assert!(settings.must_change_password);
    }

    #[tokio::test]
    async fn restart_rotates_unfinished_bootstrap_password_and_clears_sessions() {
        let pool = test_db().await;
        let config = AuthConfig {
            enabled: true,
            session_secret: "secret".to_string(),
            init_admin_username: None,
            init_admin_password: None,
        };

        AuthService::new(config.clone(), pool.clone())
            .await
            .unwrap();
        let initial = load_auth_settings(&pool).await.unwrap().unwrap();
        let stale_session = AuthSessionRecord {
            id: Uuid::now_v7(),
            session_token_hash: "stale".to_string(),
            created_at: Utc::now(),
            last_seen_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        insert_auth_session(&pool, &stale_session).await.unwrap();

        AuthService::new(config, pool.clone()).await.unwrap();

        let rotated = load_auth_settings(&pool).await.unwrap().unwrap();
        assert_eq!(rotated.admin_username, initial.admin_username);
        assert!(rotated.must_change_password);
        assert_ne!(rotated.password_hash, initial.password_hash);
        assert!(
            load_auth_session_by_hash(&pool, "stale")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn update_credentials_replaces_sessions_atomically() {
        let pool = test_db().await;
        let service = AuthService::new(
            AuthConfig {
                enabled: true,
                session_secret: "secret".to_string(),
                init_admin_username: Some("admin".to_string()),
                init_admin_password: Some("password123".to_string()),
            },
            pool.clone(),
        )
        .await
        .unwrap();

        let previous_login = service
            .login("admin", "password123")
            .await
            .unwrap()
            .unwrap();
        let previous_raw_token = service
            .verify_signed_cookie(&previous_login.cookie_value)
            .unwrap();

        let replacement = service
            .update_credentials("root", "new-password")
            .await
            .unwrap();
        let replacement_raw_token = service
            .verify_signed_cookie(&replacement.cookie_value)
            .unwrap();

        let settings = load_auth_settings(&pool).await.unwrap().unwrap();
        assert_eq!(settings.admin_username, "root");
        assert!(!settings.must_change_password);
        assert_eq!(settings.password_changed_at, Some(settings.updated_at));
        assert!(
            load_auth_session_by_hash(&pool, &hash_value(&previous_raw_token))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            load_auth_session_by_hash(&pool, &hash_value(&replacement_raw_token))
                .await
                .unwrap()
                .is_some()
        );

        assert!(
            service
                .authenticate_request(Some(&previous_login.cookie_value), false)
                .await
                .unwrap()
                .is_none()
        );

        let replacement_session = service
            .authenticate_request(Some(&replacement.cookie_value), false)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(replacement_session.username, "root");
        assert!(!replacement_session.must_change_password);
    }
}
