use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait, IntoActiveModel,
    TransactionTrait, TryIntoModel,
};
use sha2::{Digest, Sha256};
use tracing::warn;

use crate::{
    app_config::AuthConfig,
    db::{self, auth_settings::SettingsResult},
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
    db: DatabaseConnection,
}

impl AuthService {
    pub(crate) async fn new(config: AuthConfig, db: DatabaseConnection) -> AppResult<Self> {
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

        db::auth_session::Entity::delete_expired_sessions(&self.db).await?;

        let settings = self.load_settings().await?;

        if settings.admin_username != username.trim() {
            return Ok(None);
        }

        if !self.verify_password(password, &settings.admin_password_hash)? {
            return Ok(None);
        }

        let (session, outcome) = self.build_login_session(settings.must_change_password);
        session.insert(&self.db).await?;

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

        db::auth_session::Entity::delete_expired_sessions(&self.db).await?;

        let settings = self.load_settings().await?;

        let Some(session) =
            db::auth_session::Entity::find_by_session_token_hash(&self.db, &hash_value(&raw_token))
                .await?
        else {
            return Ok(None);
        };

        if rolling {
            let mut session = session.clone().into_active_model();
            session.expires_at = Set(Utc::now() + Duration::hours(24));
            session.save(&self.db).await?;
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

        db::auth_session::Entity::delete_expired_sessions(&self.db).await?;
        if let Some(raw_token) = cookie_value.and_then(|value| self.verify_signed_cookie(value))
            && let Some(session) = db::auth_session::Entity::find_by_session_token_hash(
                &self.db,
                &hash_value(&raw_token),
            )
            .await?
        {
            db::auth_session::Entity::delete_by_id(session.id)
                .exec(&self.db)
                .await?;
        }

        Ok(())
    }

    pub(crate) async fn update_credentials(
        &self,
        username: &str,
        new_password: &str,
    ) -> AppResult<LoginOutcome> {
        let settings = self.load_settings().await?;

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

        let password_hash = hash_password(new_password)?;
        let (session, outcome) = self.build_login_session(false);
        let tx = self.db.begin().await?;

        let mut settings = settings.into_active_model();

        settings.admin_username = Set(trimmed_username.to_string());
        settings.admin_password_hash = Set(password_hash.clone());
        let settings = settings.save(&tx).await?.try_into_model()?;

        db::auth_session::Entity::delete_many()
            .exec(&self.db)
            .await?;

        session.insert(&tx).await?;

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
        let settings = self.load_settings().await?;
        self.verify_password(password, &settings.admin_password_hash)
    }

    async fn bootstrap_if_needed(&self, config: &AuthConfig) -> AppResult<()> {
        let tx = self.db.begin().await?;
        let settings = db::auth_settings::Entity::get_settings(&tx).await?;
        let settings = match settings {
            SettingsResult::Existing(model) => model,
            SettingsResult::Bootstrapped(model) => {
                let mut model = model.into_active_model();
                if let Some(user) = config.init_admin_username.as_deref() {
                    model.admin_username = Set(user.to_string());
                }
                model.save(&tx).await?.try_into_model()?
            }
        };

        if settings.must_change_password {
            let mut settings = settings.into_active_model();
            if let Some(password) = config.init_admin_password.as_deref() {
                settings.admin_password_hash = Set(hash_password(password)?);
            } else {
                settings.admin_password_hash = Set(hash_password(&random_token(18))?);
            }
            settings.save(&tx).await?;

            return Ok(());
        }

        tx.commit().await?;

        Ok(())
    }

    async fn load_settings(&self) -> AppResult<db::auth_settings::Model> {
        let settings = db::auth_settings::Entity::get_settings(&self.db).await?;
        Ok(settings.into_model())
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

    fn build_login_session(
        &self,
        must_change_password: bool,
    ) -> (db::auth_session::ActiveModel, LoginOutcome) {
        let raw_token = random_token(32);
        let expires_at = Utc::now() + Duration::hours(24);

        let session = db::auth_session::ActiveModel {
            session_token_hash: Set(hash_value(&raw_token)),
            expires_at: Set(expires_at),
            ..Default::default()
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
