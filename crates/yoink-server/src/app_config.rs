use std::path::PathBuf;

use better_config::{EnvConfig, env};
use thiserror::Error;

use yoink_shared::Quality;

const DEFAULT_MUSIC_ROOT: &str = "./music";
const DEFAULT_DATABASE_URL: &str = "sqlite:./yoink.db?mode=rwc";
const DEFAULT_SITE_ROOT: &str = "target/site";
const DEFAULT_LOG_FORMAT: &str = "pretty";
const DEFAULT_SLSKD_BASE_URL: &str = "http://127.0.0.1:5030";
const DEFAULT_SLSKD_DOWNLOADS_DIR: &str = "./development/slskd-data/downloads";

#[derive(Debug, Error)]
pub(crate) enum AppConfigError {
    #[error(transparent)]
    Parse(#[from] better_config::Error),
    #[error("{0}")]
    Validation(String),
}

#[derive(Debug, Clone)]
pub(crate) struct AuthConfig {
    pub(crate) enabled: bool,
    pub(crate) session_secret: String,
    pub(crate) init_admin_username: Option<String>,
    pub(crate) init_admin_password: Option<String>,
}

#[derive(Debug)]
#[env(EnvConfig)]
struct RawAppConfig {
    /// Base URL for the Tidal hifi-api proxy.
    #[conf(from = "TIDAL_API_BASE_URL", default = "")]
    pub(crate) tidal_api_base_url: String,

    /// Whether the Tidal provider is enabled. Defaults to true.
    #[conf(from = "TIDAL_ENABLED", default = "true")]
    pub(crate) tidal_enabled: bool,

    /// Whether the MusicBrainz metadata provider is enabled. Defaults to true.
    #[conf(from = "MUSICBRAINZ_ENABLED", default = "true")]
    pub(crate) musicbrainz_enabled: bool,

    /// Whether the Deezer metadata provider is enabled. Defaults to true.
    #[conf(from = "DEEZER_ENABLED", default = "true")]
    pub(crate) deezer_enabled: bool,

    /// Whether the SoulSeek download source is enabled. Defaults to false.
    #[conf(from = "SOULSEEK_ENABLED", default = "false")]
    pub(crate) soulseek_enabled: bool,

    /// Base URL for the slskd REST API.
    #[conf(from = "SLSKD_BASE_URL", default = "http://127.0.0.1:5030")]
    pub(crate) slskd_base_url: String,

    /// Optional slskd web username for JWT auth.
    #[conf(from = "SLSKD_USERNAME", default = "")]
    pub(crate) slskd_username: String,

    /// Optional slskd web password for JWT auth.
    #[conf(from = "SLSKD_PASSWORD", default = "")]
    pub(crate) slskd_password: String,

    /// Local path to slskd downloads directory.
    #[conf(
        from = "SLSKD_DOWNLOADS_DIR",
        default = "./development/slskd-data/downloads"
    )]
    pub(crate) slskd_downloads_dir: String,

    #[conf(from = "MUSIC_ROOT", default = "./music")]
    pub(crate) music_root: String,

    #[conf(from = "DEFAULT_QUALITY", default = "LOSSLESS")]
    default_quality: Quality,

    #[conf(from = "DATABASE_URL", default = "sqlite:./yoink.db?mode=rwc")]
    pub(crate) database_url: String,

    #[conf(from = "LEPTOS_SITE_ROOT", default = "target/site")]
    pub(crate) leptos_site_root: String,

    #[conf(from = "LOG_FORMAT", default = "pretty")]
    pub(crate) log_format: String,

    #[conf(from = "DOWNLOAD_LYRICS", default = "false")]
    pub(crate) download_lyrics: bool,

    /// Maximum number of tracks to download in parallel per album job.
    #[conf(from = "DOWNLOAD_MAX_PARALLEL_TRACKS", default = "1")]
    download_max_parallel_tracks: usize,

    #[conf(from = "AUTH_DISABLED", default = "false")]
    auth_disabled: bool,

    #[conf(from = "AUTH_SESSION_SECRET", default = "")]
    auth_session_secret: String,

    #[conf(from = "AUTH_INIT_ADMIN_USERNAME", default = "")]
    auth_init_admin_username: String,

    #[conf(from = "AUTH_INIT_ADMIN_PASSWORD", default = "")]
    auth_init_admin_password: String,
}

#[derive(Debug)]
pub(crate) struct AppConfig {
    pub(crate) tidal_api_base_url: String,
    pub(crate) tidal_enabled: bool,
    pub(crate) musicbrainz_enabled: bool,
    pub(crate) deezer_enabled: bool,
    pub(crate) soulseek_enabled: bool,
    pub(crate) slskd_base_url: String,
    pub(crate) slskd_username: String,
    pub(crate) slskd_password: String,
    pub(crate) slskd_downloads_dir: String,
    pub(crate) music_root: String,
    pub(crate) default_quality: Quality,
    pub(crate) database_url: String,
    pub(crate) leptos_site_root: String,
    pub(crate) log_format: String,
    pub(crate) download_lyrics: bool,
    pub(crate) download_max_parallel_tracks: usize,
    pub(crate) auth: AuthConfig,
}

impl AppConfig {
    pub(crate) fn from_env() -> Result<Self, AppConfigError> {
        let raw = RawAppConfig::builder().build()?;
        Self::from_raw(raw)
    }

    pub(crate) fn music_root_path(&self) -> PathBuf {
        PathBuf::from(&self.music_root)
    }

    fn from_raw(raw: RawAppConfig) -> Result<Self, AppConfigError> {
        let tidal_api_base_url = normalize_string_opt(&raw.tidal_api_base_url)
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_default();
        let slskd_base_url = normalize_string(&raw.slskd_base_url, DEFAULT_SLSKD_BASE_URL)
            .trim_end_matches('/')
            .to_string();
        let slskd_downloads_dir =
            normalize_string(&raw.slskd_downloads_dir, DEFAULT_SLSKD_DOWNLOADS_DIR);
        let slskd_username = normalize_string_opt(&raw.slskd_username).unwrap_or_default();
        let slskd_password = normalize_string_opt(&raw.slskd_password).unwrap_or_default();
        let music_root = normalize_string(&raw.music_root, DEFAULT_MUSIC_ROOT);
        let default_quality = raw.default_quality;
        let database_url = normalize_string(&raw.database_url, DEFAULT_DATABASE_URL);
        let leptos_site_root = normalize_string(&raw.leptos_site_root, DEFAULT_SITE_ROOT);
        let log_format = normalize_string(&raw.log_format, DEFAULT_LOG_FORMAT).to_ascii_lowercase();
        let download_max_parallel_tracks = raw.download_max_parallel_tracks.clamp(1, 16);
        let auth_session_secret =
            normalize_string_opt(&raw.auth_session_secret).unwrap_or_default();
        let init_admin_username = normalize_string_opt(&raw.auth_init_admin_username);
        let init_admin_password = normalize_string_opt(&raw.auth_init_admin_password);

        let auth = Self::build_auth_config(
            raw.auth_disabled,
            auth_session_secret,
            init_admin_username,
            init_admin_password,
        )?;

        Ok(Self {
            tidal_api_base_url,
            tidal_enabled: raw.tidal_enabled,
            musicbrainz_enabled: raw.musicbrainz_enabled,
            deezer_enabled: raw.deezer_enabled,
            soulseek_enabled: raw.soulseek_enabled,
            slskd_base_url,
            slskd_username,
            slskd_password,
            slskd_downloads_dir,
            music_root,
            default_quality,
            database_url,
            leptos_site_root,
            log_format,
            download_lyrics: raw.download_lyrics,
            download_max_parallel_tracks,
            auth,
        })
    }

    fn build_auth_config(
        auth_disabled: bool,
        session_secret: String,
        init_admin_username: Option<String>,
        init_admin_password: Option<String>,
    ) -> Result<AuthConfig, AppConfigError> {
        if !auth_disabled && session_secret.is_empty() {
            return Err(AppConfigError::Validation(
                "AUTH_SESSION_SECRET is required when AUTH_DISABLED=false".to_string(),
            ));
        }

        match (&init_admin_username, &init_admin_password) {
            (Some(_), Some(_)) | (None, None) => {}
            _ => {
                return Err(AppConfigError::Validation(
                    "AUTH_INIT_ADMIN_USERNAME and AUTH_INIT_ADMIN_PASSWORD must both be set or both be unset"
                        .to_string(),
                ));
            }
        }

        Ok(AuthConfig {
            enabled: !auth_disabled,
            session_secret,
            init_admin_username,
            init_admin_password,
        })
    }
}

fn normalize_string(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_string_opt(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// Helper: set env vars, run closure, then clean up.
    fn with_env_vars<F: FnOnce()>(vars: &[(&str, &str)], f: F) {
        for (k, v) in vars {
            // SAFETY: tests using this helper are marked #[serial] so no
            // concurrent env mutation can occur.
            unsafe { std::env::set_var(k, v) };
        }
        f();
        for (k, _) in vars {
            unsafe { std::env::remove_var(k) };
        }
    }

    #[test]
    #[serial]
    fn uses_defaults_for_empty_values() {
        with_env_vars(
            &[
                ("MUSIC_ROOT", ""),
                ("DEFAULT_QUALITY", "   "),
                ("AUTH_DISABLED", "true"),
            ],
            || {
                let raw = RawAppConfig::builder().build().expect("config parse");
                let cfg = AppConfig::from_raw(raw).expect("config normalize");

                assert_eq!(cfg.music_root, DEFAULT_MUSIC_ROOT);
                assert_eq!(cfg.default_quality, Quality::Lossless);
            },
        );
    }

    #[test]
    #[serial]
    fn falls_back_for_invalid_quality_values() {
        with_env_vars(
            &[
                ("DEFAULT_QUALITY", "not-real-quality"),
                ("AUTH_DISABLED", "true"),
            ],
            || {
                let raw = RawAppConfig::builder().build().expect("config parse");
                let cfg = AppConfig::from_raw(raw).expect("config normalize");

                assert_eq!(cfg.default_quality, Quality::Lossless);
            },
        );
    }

    #[test]
    #[serial]
    fn trims_base_url_trailing_slash() {
        with_env_vars(
            &[
                ("TIDAL_API_BASE_URL", "http://localhost:8000///"),
                ("AUTH_DISABLED", "true"),
            ],
            || {
                let raw = RawAppConfig::builder().build().expect("config parse");
                let cfg = AppConfig::from_raw(raw).expect("config normalize");

                assert_eq!(cfg.tidal_api_base_url, "http://localhost:8000");
            },
        );
    }

    #[test]
    #[serial]
    fn requires_session_secret_when_auth_enabled() {
        with_env_vars(
            &[("AUTH_DISABLED", "false"), ("AUTH_SESSION_SECRET", "   ")],
            || {
                let raw = RawAppConfig::builder().build().expect("config parse");
                let err = AppConfig::from_raw(raw).expect_err("expected validation error");

                assert!(
                    err.to_string()
                        .contains("AUTH_SESSION_SECRET is required when AUTH_DISABLED=false")
                );
            },
        );
    }

    #[test]
    #[serial]
    fn allows_missing_session_secret_when_auth_disabled() {
        with_env_vars(
            &[("AUTH_DISABLED", "true"), ("AUTH_SESSION_SECRET", "   ")],
            || {
                let raw = RawAppConfig::builder().build().expect("config parse");
                let cfg = AppConfig::from_raw(raw).expect("config normalize");

                assert!(!cfg.auth.enabled);
                assert!(cfg.auth.session_secret.is_empty());
            },
        );
    }

    #[test]
    #[serial]
    fn rejects_partial_init_admin_config() {
        with_env_vars(
            &[
                ("AUTH_SESSION_SECRET", "session-secret"),
                ("AUTH_INIT_ADMIN_USERNAME", "admin"),
            ],
            || {
                let raw = RawAppConfig::builder().build().expect("config parse");
                let err = AppConfig::from_raw(raw).expect_err("expected validation error");

                assert!(err.to_string().contains(
                    "AUTH_INIT_ADMIN_USERNAME and AUTH_INIT_ADMIN_PASSWORD must both be set or both be unset"
                ));
            },
        );
    }
}
